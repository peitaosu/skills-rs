mod body;
mod error;
mod script;
mod filter;
mod model;
mod parse;
mod prompt;
mod validate;

#[cfg(feature = "runtime")]
mod allowed_tools;
#[cfg(feature = "runtime")]
mod activate;
#[cfg(feature = "runtime")]
mod catalog;
#[cfg(feature = "runtime")]
mod discovery;
#[cfg(feature = "runtime")]
mod session;

#[cfg(feature = "runtime")]
pub use allowed_tools::AllowedToolsPolicy;
#[cfg(feature = "runtime")]
pub use activate::{
    list_resources, resolve_under_skill_dir, safe_relative_path, ResourceListing, SkillActivator,
};
#[cfg(feature = "runtime")]
pub use catalog::{CatalogPlacement, SkillCatalog};
#[cfg(feature = "runtime")]
pub use discovery::{
    ancestor_directories, discover_and_merge, discover_and_merge_from, discover_from,
    find_git_root, format_diagnostics, home_dir, merge_skills, resolve_skill_dir_after_unpack,
    unpack_skill_archive, xdg_skills_dir, DiscoveryConfig, DiscoveryResult, FilesystemDiscovery, MultiSkillSource,
    RegistryManifest, RegistrySkillRef, RemoteRegistrySource, SkillSource, StaticSkillSource,
};
#[cfg(feature = "runtime")]
pub use filter::{
    AllowAll, CompositeFilter, DisabledSkillsFilter, HideModelDisabled, PermissionDeniedFilter,
    SkillFilter, TrustedRootsFilter,
};
pub use error::{ParseError, SkillError, ValidationError};
pub use model::{
    parse_truthy, ActivatedSkill, ActivationMode, ActivationOptions, CollisionWarning, Diagnostic,
    Severity, SkillDocument, SkillEntry, SkillProperties, SkillScope, WrapStyle,
    METADATA_DISABLE_MODEL_INVOCATION,
};
pub use parse::{
    find_skill_md, parse_frontmatter, parse_frontmatter_strict, read_document, read_document_at,
    read_properties,
};
pub use prompt::{to_prompt, to_prompt_from_dirs, to_prompt_from_properties};
#[cfg(feature = "runtime")]
pub use session::SkillSession;
pub use validate::{
    is_runtime_loadable, validate, validate_advisories, validate_lenient, validate_metadata,
    validate_metadata_with_policy, validate_with_policy, NameValidationPolicy,
    MAX_COMPATIBILITY_LENGTH, MAX_DESCRIPTION_LENGTH, MAX_SKILL_NAME_LENGTH,
};
pub use body::{
    validate_body_advisories, MAX_FILE_REFERENCE_DEPTH, RECOMMENDED_SKILL_BODY_LINES,
    RECOMMENDED_SKILL_BODY_TOKENS,
};
pub use script::validate_scripts_advisories;

pub type Result<T> = error::Result<T>;
