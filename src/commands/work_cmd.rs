// use anyhow::Result;
// use k8s_openapi::api::batch::v1::Job;
// use kube::{
//     api::{Api, ListParams},
//     Client,
// };
// use nebulous::mutation::Mutation;
// use nebulous::query::Query;
// use sea_orm::DatabaseConnection;
// use tracing::info;

// pub async fn execute_report_jobs(db: &DatabaseConnection, namespace: &str) -> Result<()> {
//     loop {
//         match _execute_report_jobs(db, namespace).await {
//             Ok(_) => {
//                 // Successfully executed
//             }
//             Err(e) => {
//                 eprintln!("Error while reporting jobs: {}", e);
//                 // Errors won't break the loop
//             }
//         }

//         // Sleep some amount of time before the next iteration
//         tokio::time::sleep(std::time::Duration::from_secs(5)).await;
//     }
// }

// pub async fn _execute_report_jobs(db: &DatabaseConnection, namespace: &str) -> Result<()> {
//     // 1) Create a Kubernetes client
//     let client = Client::try_default().await?;

//     // 2) Set up an Api for fetching jobs in your namespace
//     let jobs_api: Api<Job> = Api::namespaced(client, namespace);

//     // 3) Filter jobs with the label "type=trainingjob" (as an example)
//     let lp = ListParams::default().labels("type=trainingjob");
//     let job_list = jobs_api.list(&lp).await?;

//     info!("job_list: {:?}", job_list);

//     // 4) For each Kubernetes Job, look up "id" from labels, then update your DB record
//     for kjob in job_list.items {
//         let job_name = kjob.metadata.name.clone().unwrap_or_default();

//         // Here’s one way to parse a user-friendly status from the "conditions" field
//         let job_phase = if let Some(status) = &kjob.status {
//             if let Some(conditions) = &status.conditions {
//                 if conditions
//                     .iter()
//                     .any(|cond| cond.type_ == "Complete" && cond.status == "True")
//                 {
//                     "Complete".to_string()
//                 } else if conditions
//                     .iter()
//                     .any(|cond| cond.type_ == "Failed" && cond.status == "True")
//                 {
//                     "Failed".to_string()
//                 } else {
//                     "Active".to_string()
//                 }
//             } else {
//                 "Unknown".to_string()
//             }
//         } else {
//             "Unknown".to_string()
//         };

//         // Look for a "job_id" label (or "id", depending on how you labeled it)
//         if let Some(job_id_label) = kjob
//             .metadata
//             .labels
//             .as_ref()
//             .and_then(|labels| labels.get("id").cloned())
//         {
//             // Find the corresponding training_job by ID in the DB
//             if let Ok(Some(db_job)) = Query::find_training_job_by_id(db, &job_id_label).await {
//                 // Update the training job's status
//                 Mutation::update_training_job_status(db, &db_job.id, &job_phase).await?;
//                 println!(
//                     "Job '{}' => Updated training job '{}' status to '{}'",
//                     job_name, db_job.id, job_phase
//                 );
//             } else {
//                 println!(
//                     "Job '{}' => no matching training job for job_id '{}'",
//                     job_name, job_id_label
//                 );
//             }
//         } else {
//             println!("Job '{}' => no 'id' label found.", job_name);
//         }
//     }

//     Ok(())
// }
