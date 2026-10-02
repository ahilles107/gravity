//! A project's shared git repository.

use bus::{now, ProjectRepo};
use rusqlite::{params, OptionalExtension};

use super::{ts, Db};

impl Db {
    pub fn project_repo(&self, project_id: &str) -> anyhow::Result<Option<ProjectRepo>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT url, branch FROM project_repo WHERE project_id = ?1",
                params![project_id],
                |r| {
                    Ok(ProjectRepo {
                        url: r.get(0)?,
                        branch: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    /// Set the project's repository, or clear it with `None`.
    pub fn set_project_repo(
        &self,
        project_id: &str,
        repo: Option<&ProjectRepo>,
    ) -> anyhow::Result<()> {
        let conn = self.lock();
        match repo {
            Some(repo) => conn.execute(
                "INSERT INTO project_repo(project_id, url, branch, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(project_id) DO UPDATE
                   SET url = excluded.url, branch = excluded.branch,
                       updated_at = excluded.updated_at",
                params![project_id, repo.url, repo.branch, ts(now())],
            )?,
            None => conn.execute(
                "DELETE FROM project_repo WHERE project_id = ?1",
                params![project_id],
            )?,
        };
        Ok(())
    }
}
