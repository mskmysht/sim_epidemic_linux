use std::sync::Arc;

use poem_openapi::types::ToJSON;
use tokio_postgres::Client;
use uuid::Uuid;

use crate::{
    app::{
        job::{self, JobState},
        task::{self, TaskState},
    },
    manager::JobId,
    worker::TaskId,
};

#[derive(Clone)]
pub struct Db(pub Arc<Client>);

impl Db {
    pub async fn insert_job(
        &self,
        config: &job::Config,
    ) -> Result<(JobId, Vec<TaskId>), tokio_postgres::Error> {
        let state = if config.iteration_count == 0 {
            JobState::Completed
        } else {
            JobState::Queued
        };
        let rows = self
            .0
            .query(
                "
                INSERT INTO job (id, state, config) VALUES (DEFAULT, $1, $2) RETURNING id
                ",
                &[&state, &config.to_json().unwrap()],
            )
            .await?;
        let job_id = JobId(rows[0].get(0));
        let statement = self
            .0
            .prepare(
                "
                INSERT INTO task (id, job_id, state) VALUES (DEFAULT, $1, $2)
                RETURNING id",
            )
            .await?;

        let mut task_ids = Vec::new();
        for _ in 0..config.iteration_count {
            let rows = self
                .0
                .query(&statement, &[&job_id.0, &TaskState::default()])
                .await?;
            task_ids.push(TaskId(rows[0].get(0)));
        }
        Ok((job_id, task_ids))
    }

    pub async fn update_task_succeeded(&self, task_id: &TaskId, worker_index: usize) {
        self.0
            .execute(
                "UPDATE task SET worker_index = $1, state = $2 WHERE id = $3",
                &[&(worker_index as i32), &TaskState::Succeeded, &task_id.0],
            )
            .await
            .unwrap();
    }

    pub async fn update_task_state(&self, task_id: &TaskId, state: &TaskState) {
        self.0
            .execute(
                "UPDATE task SET state = $1 WHERE id = $2",
                &[state, &task_id.0],
            )
            .await
            .unwrap();
    }

    pub async fn update_job_state(&self, job_id: &JobId, state: &JobState) {
        self.0
            .execute(
                "UPDATE job SET state = $1 WHERE id = $2",
                &[state, &job_id.0],
            )
            .await
            .unwrap();
    }

    pub async fn get_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<task::Task>, tokio_postgres::Error> {
        let rs = self
            .0
            .query("SELECT id, state FROM task WHERE id = $1", &[&task_id.0])
            .await?;
        let Some(r) = rs.get(0) else { return Ok(None) };
        let id: Uuid = r.get(0);
        let state: task::TaskState = r.get(1);
        Ok(Some(task::Task {
            id: id.to_string(),
            state,
        }))
    }

    async fn get_tasks(&self, job_id: &JobId) -> Vec<task::Task> {
        let mut tasks = Vec::new();
        for r in self
            .0
            .query("SELECT id, state FROM task WHERE job_id = $1", &[&job_id.0])
            .await
            .unwrap()
        {
            let id: Uuid = r.get(0);
            let state: task::TaskState = r.get(1);
            tasks.push(task::Task {
                id: id.to_string(),
                state,
            })
        }
        tasks
    }

    pub async fn get_job(&self, id: &JobId) -> anyhow::Result<Option<job::Job>> {
        let rs = self
            .0
            .query("SELECT state, config FROM job WHERE id = $1", &[&id.0])
            .await?;
        let Some(r) = rs.get(0) else { return Ok(None) };
        let state: job::JobState = r.get(0);
        let config_json: postgres_types::Json<serde_json::Value> = r.get(1);
        let config: job::Config =
            poem_openapi::types::ParseFromJSON::parse_from_json(Some(config_json.0))
                .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;

        Ok(Some(job::Job {
            id: id.to_string(),
            state,
            config,
            tasks: self.get_tasks(id).await,
        }))
    }

    pub async fn get_jobs(&self) -> anyhow::Result<Vec<job::Job>> {
        let mut jobs = Vec::new();
        for r in self
            .0
            .query("SELECT id, state, config FROM job", &[])
            .await?
        {
            let id: Uuid = r.get(0);
            let state: job::JobState = r.get(1);
            let config_json: postgres_types::Json<serde_json::Value> = r.get(2);
            let config: job::Config =
                poem_openapi::types::ParseFromJSON::parse_from_json(Some(config_json.0))
                    .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;

            jobs.push(job::Job {
                id: id.to_string(),
                state,
                config,
                tasks: self.get_tasks(&JobId(id)).await,
            })
        }
        Ok(jobs)
    }

    pub async fn get_all_tasks_with_stats(
        &self,
        id: &JobId,
    ) -> Result<Vec<(TaskId, usize)>, tokio_postgres::Error> {
        let mut v = Vec::new();
        for r in self
            .0
            .query(
                "SELECT id, worker_index FROM task WHERE job_id = $1 AND worker_index IS NOT NULL",
                &[&id.0],
            )
            .await?
        {
            let task_id: Uuid = r.get(0);
            let worker_index: i32 = r.get(1);
            v.push((TaskId(task_id), worker_index as usize));
        }
        Ok(v)
    }

    pub async fn delete_job(&self, id: &JobId) -> Result<(), tokio_postgres::Error> {
        self.0
            .execute("DELETE FROM task WHERE job_id = $1", &[&id.0])
            .await?;
        self.0
            .execute("DELETE FROM job WHERE id = $1", &[&id.0])
            .await?;
        tracing::info!("removed job {} from DB", id);
        Ok(())
    }

    pub async fn get_worker_index(
        &self,
        id: &TaskId,
    ) -> Result<Option<usize>, tokio_postgres::Error> {
        let rs = self
            .0
            .query(
                "SELECT worker_index FROM task WHERE id = $1 AND worker_index IS NOT NULL",
                &[&id.0],
            )
            .await?;
        let Some(r) = rs.get(0) else {return Ok(None)};
        let i = r.get::<_, i32>(0);
        Ok(Some(i as usize))
    }
}
