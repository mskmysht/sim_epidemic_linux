pub mod job;

use poem_openapi::Tags;

#[derive(Tags)]
enum ApiTags {
    /// Operations about job
    Job,
}
