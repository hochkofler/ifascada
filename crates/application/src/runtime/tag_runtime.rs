use crate::runtime::live_state::TagState;
use crate::runtime::tag_pipeline::{TagValuePipeline, build_pipeline_for_tag};
use domain::tag::{Tag, TagValue};
use std::sync::Arc;

pub struct TagRuntime {
    pub definition: Arc<Tag>,
    pipeline: Arc<dyn TagValuePipeline>,
}

impl TagRuntime {
    pub fn new(tag: Tag) -> Self {
        let pipeline = build_pipeline_for_tag(&tag);
        Self::new_with_pipeline(tag, pipeline)
    }

    pub fn new_with_pipeline(tag: Tag, pipeline: Arc<dyn TagValuePipeline>) -> Self {
        Self {
            definition: Arc::new(tag),
            pipeline,
        }
    }

    pub fn process_raw_value(&self, raw_value: TagValue) -> TagState {
        self.pipeline.process(&self.definition, raw_value)
    }
}
