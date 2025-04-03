use anyhow::Result;
use aws_config::SdkConfig;
use aws_sdk_iam::Client as IamClient;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client as S3Client;
use serde_json::json;

pub struct S3ClientInternal {
    client: S3Client,
    bucket: String,
    base_path: String,
}

impl S3ClientInternal {
    pub fn new(
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        namespace: &str,
        name: &str,
    ) -> Result<Self> {
        let credentials = Credentials::new(
            access_key,
            secret_key,
            None, // No session token needed for permanent credentials
            None, // No expiration
            "permanent-credentials",
        );

        let config = aws_sdk_s3::Config::builder()
            .region(Region::new("us-east-1")) // adjust as needed
            .credentials_provider(credentials)
            .build();

        let client = S3Client::from_conf(config);
        let base_path = format!("data/{}/{}", namespace, name);

        Ok(Self {
            client,
            bucket: bucket.to_string(),
            base_path,
        })
    }

    // Helper method to construct full path
    fn full_path(&self, key: &str) -> String {
        format!("{}/{}", self.base_path, key.trim_matches('/'))
    }

    // Example methods for S3 operations
    pub async fn put_object(&self, key: &str, data: Vec<u8>) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.full_path(key))
            .body(data.into())
            .send()
            .await?;
        Ok(())
    }

    pub async fn get_object(&self, key: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.full_path(key))
            .send()
            .await?;

        Ok(response.body.collect().await?.into_bytes().to_vec())
    }

    pub async fn list_objects(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let prefix = match prefix {
            Some(p) => format!("{}/{}", self.base_path, p.trim_matches('/')),
            None => self.base_path.clone(),
        };

        let response = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .send()
            .await?;

        let keys: Vec<String> = response
            .contents()
            .iter()
            .filter_map(|obj| obj.key.clone())
            .collect();

        Ok(keys)
    }
}

pub struct IamCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub username: String,
}

pub async fn create_s3_scoped_user(
    bucket_name: &str,
    namespace: &str,
    name: &str,
) -> Result<IamCredentials> {
    let config = SdkConfig::builder()
        .region(Region::new("us-east-1"))
        .build();
    let client = IamClient::new(&config);

    // Create a unique username
    let username = format!("s3-scoped-{}-{}", namespace, name);

    // Create the IAM user
    client.create_user().user_name(&username).send().await?;

    // Create the policy document
    let policy_document = json!({
        "Version": "2012-10-17",
        "Statement": [
            {
                "Effect": "Allow",
                "Action": [
                    "s3:GetObject",
                    "s3:PutObject",
                    "s3:ListBucket"
                ],
                "Resource": [
                    format!("arn:aws:s3:::{}/data/{}", bucket_name, namespace),
                    format!("arn:aws:s3:::{}/data/{}", bucket_name, namespace)
                ]
            },
            {
                "Effect": "Allow",
                "Action": "s3:ListBucket",
                "Resource": format!("arn:aws:s3:::{}", bucket_name),
                "Condition": {
                    "StringLike": {
                        "s3:prefix": [
                            format!("data/{}", namespace),
                            format!("data/{}", namespace)
                        ]
                    }
                }
            }
        ]
    });

    // Create the policy
    let policy_name = format!("s3-scope-{}-{}", namespace, name);
    let policy_response = client
        .create_policy()
        .policy_name(&policy_name)
        .policy_document(policy_document.to_string())
        .send()
        .await?;

    // Attach the policy to the user
    client
        .attach_user_policy()
        .user_name(&username)
        .policy_arn(policy_response.policy().unwrap().arn().unwrap())
        .send()
        .await?;

    // Create access key for the user
    let key_response = client
        .create_access_key()
        .user_name(&username)
        .send()
        .await?;

    let access_key = key_response.access_key().unwrap();

    Ok(IamCredentials {
        access_key_id: access_key.access_key_id().to_string(),
        secret_access_key: access_key.secret_access_key().to_string(),
        username,
    })
}

pub async fn delete_s3_scoped_user(namespace: &str, name: &str) -> Result<()> {
    let config = SdkConfig::builder()
        .region(Region::new("us-east-1"))
        .build();
    let client = IamClient::new(&config);

    let username = format!("s3-scoped-{}-{}", namespace, name);
    let policy_name = format!("s3-scope-{}-{}", namespace, name);

    // First, list and delete all access keys for the user
    let keys = client
        .list_access_keys()
        .user_name(&username)
        .send()
        .await?;
    for key in keys.access_key_metadata() {
        client
            .delete_access_key()
            .user_name(&username)
            .access_key_id(key.access_key_id().unwrap_or_default())
            .send()
            .await?;
    }

    // Get the policy ARN and detach it from the user
    let policies = client
        .list_user_policies()
        .user_name(&username)
        .send()
        .await?;
    for policy in policies.policy_names() {
        client
            .delete_user_policy()
            .user_name(&username)
            .policy_name(policy)
            .send()
            .await?;
    }

    // Delete the policy
    let policies = client
        .list_policies()
        .scope(aws_sdk_iam::types::PolicyScopeType::Local)
        .path_prefix("/")
        .send()
        .await?;

    for policy in policies.policies() {
        if policy.policy_name() == Some(&policy_name) {
            client
                .delete_policy()
                .policy_arn(policy.arn().unwrap())
                .send()
                .await?;
            break;
        }
    }

    // Finally, delete the user
    client.delete_user().user_name(&username).send().await?;

    Ok(())
}
