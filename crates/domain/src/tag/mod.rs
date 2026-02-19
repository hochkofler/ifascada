mod aggregate;
mod entity;
mod pipeline;
mod quality;
mod repository;
mod status;
mod tag_id;
mod update_mode;
mod value; // NEW
mod value_type;

pub use aggregate::Tag;
pub use entity::Tag as TagEntity;
pub use pipeline::{
    ParserConfig, PipelineConfig, ScalingConfig, ValidatorConfig, ValueParser, ValueValidator,
};
pub use quality::TagQuality;
pub use repository::TagRepository;
pub use status::TagStatus;
pub use tag_id::TagId;
pub use update_mode::TagUpdateMode;
pub use value::TagValue; // NEW
pub use value_type::TagValueType;
