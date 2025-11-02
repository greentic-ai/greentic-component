#![forbid(unsafe_code)]

pub mod store;

pub use store::{
    CompatError, CompatPolicy, ComponentBytes, ComponentId, ComponentLocator, ComponentStore,
    MetaInfo, SourceId,
};
