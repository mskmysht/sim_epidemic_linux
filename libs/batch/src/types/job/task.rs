use poem_openapi::Enum;

#[derive(Enum, Clone)]
enum Status {
    Pending,
    Assigned,
    Running,
    Failed,
    Succeeded,
}
