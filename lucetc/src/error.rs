use failure::{Backtrace, Context, Error, Fail};
use pwasm_validation;
use std::fmt::{self, Display};

#[derive(Debug)]
pub struct LucetcError {
    inner: Context<LucetcErrorKind>,
}

impl From<Context<LucetcErrorKind>> for LucetcError {
    fn from(inner: Context<LucetcErrorKind>) -> LucetcError {
        LucetcError { inner }
    }
}

impl From<LucetcErrorKind> for LucetcError {
    fn from(kind: LucetcErrorKind) -> LucetcError {
        LucetcError {
            inner: Context::new(kind),
        }
    }
}

impl LucetcError {
    pub fn get_context(&self) -> &LucetcErrorKind {
        self.inner.get_context()
    }
}

impl Fail for LucetcError {
    fn cause(&self) -> Option<&Fail> {
        self.inner.cause()
    }
    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for LucetcError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl From<Error> for LucetcError {
    fn from(e: Error) -> LucetcError {
        e.context(LucetcErrorKind::UnknownKind).into()
    }
}

impl From<pwasm_validation::Error> for LucetcError {
    fn from(e: pwasm_validation::Error) -> LucetcError {
        e.context(LucetcErrorKind::Validation).into()
    }
}

#[derive(Debug, Fail, PartialEq, Eq, Clone)]
pub enum LucetcErrorKind {
    #[fail(display = "Translating module")]
    TranslatingModule,
    #[fail(display = "Module data")]
    ModuleData,
    #[fail(display = "Metadata Serializer")] // specifically non-ModuleData; this will go away soon
    MetadataSerializer,
    #[fail(display = "Function {}", _0)]
    Function(String),
    #[fail(display = "Table {}", _0)]
    Table(String),
    #[fail(display = "Memory Specs")]
    MemorySpecs,
    #[fail(display = "Validation")]
    Validation,
    #[fail(display = "Output")]
    Output,

    #[fail(display = "Unsupported: {}", _0)]
    Unsupported(String),

    #[fail(display = "Unknown error:")]
    UnknownKind,
}
