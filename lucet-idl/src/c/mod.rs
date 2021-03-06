#![allow(dead_code)]
#![allow(unused_variables)]

mod accessors;
mod alias;
mod catom;
mod r#enum;
mod macros;
mod prelude;
mod r#struct;

pub(crate) use self::catom::*;
use crate::backend::*;
use crate::cache::*;
use crate::errors::*;
use crate::generator::{Generator, Hierarchy};
use crate::module::*;
use crate::module::{DataTypeEntry, DataTypeRef, Module};
use crate::pretty_writer::*;
use crate::target::*;
use std::io::prelude::*;

#[derive(Clone, Debug)]
struct CTypeInfo<'t> {
    /// The native type name
    type_name: String,
    /// Alignment rules for that type
    type_align: usize,
    /// The native type size
    type_size: usize,
    /// How many pointer indirections are required to get to the atomic type
    indirections: usize,
    /// The leaf type node
    leaf_data_type_ref: &'t DataTypeRef,
}

/// Generator for the C backend
pub struct CGenerator {
    pub target: Target,
    pub backend_config: BackendConfig,
}

impl<W: Write> Generator<W> for CGenerator {
    fn gen_prelude(&mut self, pretty_writer: &mut PrettyWriter<W>) -> Result<(), IDLError> {
        pretty_writer
            .eob()?
            .write_line(b"// ---------- Prelude ----------")?
            .eob()?;
        prelude::generate(pretty_writer, self.target, self.backend_config)?;
        Ok(())
    }

    fn gen_type_header(
        &mut self,
        _module: &Module,
        _cache: &mut Cache,
        pretty_writer: &mut PrettyWriter<W>,
        data_type_entry: &DataTypeEntry<'_>,
    ) -> Result<(), IDLError> {
        pretty_writer
            .eob()?
            .write_line(
                format!("// ---------- {} ----------", data_type_entry.name.name).as_bytes(),
            )?
            .eob()?;
        Ok(())
    }

    // The most important thing in alias generation is to cache the size
    // and alignment rules of what it ultimately points to
    fn gen_alias(
        &mut self,
        module: &Module,
        cache: &mut Cache,
        pretty_writer: &mut PrettyWriter<W>,
        data_type_entry: &DataTypeEntry<'_>,
    ) -> Result<(), IDLError> {
        alias::generate(self, module, cache, pretty_writer, data_type_entry)
    }

    fn gen_struct(
        &mut self,
        module: &Module,
        cache: &mut Cache,
        pretty_writer: &mut PrettyWriter<W>,
        data_type_entry: &DataTypeEntry<'_>,
    ) -> Result<(), IDLError> {
        r#struct::generate(self, module, cache, pretty_writer, data_type_entry)
    }

    // Enums generate both a specific typedef, and a traditional C-style enum
    // The typedef is required to use a native type which is consistent across all architectures
    fn gen_enum(
        &mut self,
        module: &Module,
        cache: &mut Cache,
        pretty_writer: &mut PrettyWriter<W>,
        data_type_entry: &DataTypeEntry<'_>,
    ) -> Result<(), IDLError> {
        r#enum::generate(self, module, cache, pretty_writer, data_type_entry)
    }

    fn gen_accessors_struct(
        &mut self,
        module: &Module,
        cache: &Cache,
        pretty_writer: &mut PrettyWriter<W>,
        data_type_entry: &DataTypeEntry<'_>,
        hierarchy: &Hierarchy,
    ) -> Result<(), IDLError> {
        accessors::r#struct::generate(
            self,
            module,
            cache,
            pretty_writer,
            data_type_entry,
            hierarchy,
        )
    }

    fn gen_accessors_enum(
        &mut self,
        module: &Module,
        cache: &Cache,
        pretty_writer: &mut PrettyWriter<W>,
        data_type_entry: &DataTypeEntry<'_>,
        hierarchy: &Hierarchy,
    ) -> Result<(), IDLError> {
        accessors::r#enum::generate(
            self,
            module,
            cache,
            pretty_writer,
            data_type_entry,
            hierarchy,
        )
    }

    fn gen_accessors_alias(
        &mut self,
        module: &Module,
        cache: &Cache,
        pretty_writer: &mut PrettyWriter<W>,
        data_type_entry: &DataTypeEntry<'_>,
        hierarchy: &Hierarchy,
    ) -> Result<(), IDLError> {
        accessors::alias::generate(
            self,
            module,
            cache,
            pretty_writer,
            data_type_entry,
            hierarchy,
        )
    }
}

impl CGenerator {
    /// Traverse a `DataTypeRef` chain, and return information
    /// about the leaf node as well as the native type to use
    /// for this data type
    fn type_info<'t>(
        &self,
        module: &'t Module,
        cache: &Cache,
        mut type_: &'t DataTypeRef,
    ) -> CTypeInfo<'t> {
        let mut indirections = 0;
        let (mut type_align, mut type_size) = (None, None);
        let mut type_name = None;
        loop {
            match &type_ {
                DataTypeRef::Ptr(to) => {
                    type_ = to.as_ref();
                    // Only keep counting indirections if we are not resolving an alias
                    if type_name.is_none() {
                        indirections += 1;
                    }
                    continue;
                }
                DataTypeRef::Atom(atom_type) => {
                    let native_atom = CAtom::from(*atom_type);
                    type_align = type_align.or_else(|| Some(native_atom.native_type_align));
                    type_size = type_size.or_else(|| Some(native_atom.native_type_size));
                    type_name =
                        type_name.or_else(|| Some(native_atom.native_type_name.to_string()));
                }
                DataTypeRef::Defined(data_type_id) => {
                    if indirections == 0 {
                        let cached = cache.load_type(*data_type_id).unwrap();
                        type_align = type_align.or_else(|| Some(cached.type_align));
                        type_size = type_size.or_else(|| Some(cached.type_size));
                    }
                    let data_type_entry = module.get_datatype(*data_type_id);
                    match data_type_entry.data_type {
                        DataType::Struct { .. } => {
                            type_name = type_name
                                .or_else(|| Some(format!("struct {}", data_type_entry.name.name)))
                        }
                        DataType::Enum { .. } => {
                            type_name = type_name.or_else(|| {
                                Some(format!(
                                    "{} /* (enum ___{}) */",
                                    data_type_entry.name.name, data_type_entry.name.name
                                ))
                            })
                        }
                        DataType::Alias { to, .. } => {
                            type_name =
                                type_name.or_else(|| Some(data_type_entry.name.name.to_string()));
                            type_ = &to;
                            continue;
                        }
                    };
                }
            }
            break;
        }
        // No matter what the base type is, pointers always have the same size
        if indirections > 0 {
            type_align = Some(CAtom::ptr().native_type_align);
            type_size = Some(CAtom::ptr().native_type_size);
        }
        CTypeInfo {
            type_name: type_name.unwrap(),
            type_align: type_align.unwrap(),
            type_size: type_size.unwrap(),
            indirections,
            leaf_data_type_ref: type_,
        }
    }

    // Return `true` if the type is an atom, an emum, or an alias to one of these
    pub fn is_type_eventually_an_atom_or_enum(&self, module: &Module, type_: &DataTypeRef) -> bool {
        let inner_type = match type_ {
            DataTypeRef::Atom(_) => return true,
            DataTypeRef::Ptr(_) => return false,
            DataTypeRef::Defined(inner_type) => inner_type,
        };
        let inner_data_type_entry = module.get_datatype(*inner_type);
        let inner_data_type = inner_data_type_entry.data_type;
        match inner_data_type {
            DataType::Struct { .. } => false,
            DataType::Enum { .. } => true,
            DataType::Alias { to, .. } => self.is_type_eventually_an_atom_or_enum(module, to),
        }
    }

    /// Return the type refererence, with aliases being resolved
    pub fn unalias<'t>(&self, module: &'t Module, type_: &'t DataTypeRef) -> &'t DataTypeRef {
        let inner_type = match type_ {
            DataTypeRef::Atom(_) | DataTypeRef::Ptr(_) => return type_,
            DataTypeRef::Defined(inner_type) => inner_type,
        };
        let inner_data_type_entry = module.get_datatype(*inner_type);
        let inner_data_type = inner_data_type_entry.data_type;
        if let DataType::Alias { to, .. } = inner_data_type {
            self.unalias(module, to)
        } else {
            type_
        }
    }

    /// Possibly add some padding so that pointers are aligned like the reference platform
    pub fn pointer_pad<W: Write>(
        &mut self,
        pretty_writer: &mut PrettyWriter<W>,
        indirections: usize,
        offset: usize,
        name: &str,
    ) -> Result<(), IDLError> {
        if indirections == 0 {
            return Ok(());
        }
        if self.target.is_reference_target() {
            return Ok(());
        }
        pretty_writer.write_line(
            format!(
                "___POINTER_PAD({}) // pad pointer `{}` at offset {} to match alignment of the reference target ({} bytes)",
                offset,
                name,
                offset,
                CAtom::ptr().native_type_align
            )
            .as_bytes(),
        )?;
        Ok(())
    }

    fn gen_accessors_for_data_type_ref<W: Write>(
        &mut self,
        module: &Module,
        cache: &Cache,
        pretty_writer: &mut PrettyWriter<W>,
        type_: &DataTypeRef,
        name: &str,
        hierarchy: &Hierarchy,
    ) -> Result<(), IDLError> {
        let type_ = self.unalias(module, type_);
        match type_ {
            DataTypeRef::Atom(atom_type) => {
                accessors::atom::generate(self, module, pretty_writer, *atom_type, &hierarchy)
            }
            DataTypeRef::Ptr(type_) => {
                accessors::ptr::generate(self, module, cache, pretty_writer, type_, &hierarchy)
            }
            DataTypeRef::Defined(data_type_id) => {
                self.gen_accessors_for_id(module, cache, pretty_writer, *data_type_id, &hierarchy)
            }
        }
    }
}
