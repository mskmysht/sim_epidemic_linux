DROP TABLE task;
DROP TABLE job;

CREATE TYPE "JobState" AS ENUM (
    'Created',
    'Queued',
    'Scheduled',
    'Running',
    'Completed'
);

CREATE TYPE "TaskState" AS ENUM (
    'Pending',
    'Assigned',
    'Running',
    'Failed',
    'Succeeded'
);

CREATE TABLE job (
    id UUID PRIMARY KEY NOT NULL DEFAULT gen_random_uuid(),
    state "JobState" NOT NULL,
    config jsonb NOT NULL
);

CREATE TABLE task (
    id UUID PRIMARY KEY NOT NULL DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES job (id),
    state "TaskState" NOT NULL,
    worker_index integer
);