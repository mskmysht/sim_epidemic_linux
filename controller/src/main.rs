use poem::{
    endpoint::make_sync, listener::TcpListener, middleware::Cors, web::Html, EndpointExt, Route,
};
use poem_openapi::{param::Query, payload::PlainText, OpenApi, OpenApiService, Tags};

#[derive(Tags)]
enum ApiTags {
    /// Operations about job
    Job,
}

struct JobApi;

#[OpenApi(prefix_path = "/user", tag = "ApiTags::Job")]
impl JobApi {
    #[oai(path = "/hello", method = "get")]
    async fn index(&self, name: Query<Option<String>>) -> PlainText<String> {
        match name.0 {
            Some(name) => PlainText(format!("hello, {}!", name)),
            None => PlainText("hello!".to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let api_service =
        OpenApiService::new(JobApi, "Hello World", "1.0").server("http://localhost:3000/api");
    let spec = api_service.spec_endpoint();
    let app = Route::new()
        .nest("/api", api_service)
        .nest("/spec.json", spec)
        .at("/", make_sync(|_| Html(include_str!("index.html"))))
        .with(Cors::new());

    poem::Server::new(TcpListener::bind("0.0.0.0:3000"))
        .run(app)
        .await
}
