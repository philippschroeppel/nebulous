use crate::proxy::authz::extract_json_path;
use openmeter::{CloudEvent, MeterClient};
use serde_json::Value;
use short_uuid::ShortUuid;
use tracing::{debug, error, warn};

pub async fn send_request_metrics(
    container_id: &str,
    meters: &[crate::models::V1Meter],
    json_body_opt: &Option<Value>,
) -> Result<(), String> {
    // Get OpenMeter configuration from environment
    let openmeter_url = std::env::var("OPENMETER_URL")
        .map_err(|_| "OPENMETER_URL environment variable not set".to_string())?;

    let openmeter_token = std::env::var("OPENMETER_TOKEN")
        .map_err(|_| "OPENMETER_TOKEN environment variable not set".to_string())?;

    let meter_client = MeterClient::new(openmeter_url, openmeter_token);

    for meter in meters {
        // Skip meters that are not for request processing
        if meter.metric == "response_value" {
            continue;
        }

        debug!("[PROXY] Meter: {meter:?}");

        // If `json_path` does not exist, warn and skip
        let Some(json_path) = &meter.json_path else {
            warn!(
                "[Metrics] No json_path for meter '{}', skipping.",
                meter.metric
            );
            continue;
        };

        // Extract value from JSON using the path
        let extracted_val = match json_body_opt {
            Some(json) => extract_json_path(json, json_path),
            None => None,
        };

        debug!("[PROXY] Extracted meter value: {extracted_val:?}");

        let Some(value) = extracted_val else {
            warn!(
                "[Metrics] Could not extract JSON for meter '{}' using json_path='{}', skipping.",
                meter.metric, json_path
            );
            continue;
        };

        let data = serde_json::json!({
            "value": value,
            "metric": meter.metric,
            "container_id": container_id,
            "currency": meter.currency,
            "cost": meter.cost,
            "unit": meter.unit,
            "kind": "Container",
            "service": "Nebulous",
        });
        debug!("[PROXY] request metrics data: {data:?}");

        // Create CloudEvent
        let cloud_event = CloudEvent {
            id: ShortUuid::generate().to_string(),
            source: "nebulous-proxy".to_string(),
            specversion: "1.0".to_string(),
            r#type: meter.metric.clone(),
            subject: container_id.to_string(),
            time: Some(chrono::Utc::now().to_rfc3339()),
            dataschema: None,
            datacontenttype: Some("application/json".to_string()),
            data: Some(data),
        };

        // Send the event to OpenMeter
        if let Err(e) = meter_client.ingest_events(&[cloud_event]).await {
            error!(
                "[Metrics] Failed to report meter {:?} for container {}: {}",
                meter, container_id, e
            );
            return Err(format!("Failed to send metrics: {}", e));
        }

        debug!(
            "[Metrics] Successfully reported meter {:?} for container {}",
            meter, container_id
        );
    }

    Ok(())
}

pub async fn send_response_metrics(
    container_id: &str,
    meters: &[crate::models::V1Meter],
    json_response: &Value,
) -> Result<(), String> {
    // Get OpenMeter configuration from environment
    let openmeter_url = std::env::var("OPENMETER_URL")
        .map_err(|_| "OPENMETER_URL environment variable not set".to_string())?;

    let openmeter_token = std::env::var("OPENMETER_TOKEN")
        .map_err(|_| "OPENMETER_TOKEN environment variable not set".to_string())?;

    let meter_client = MeterClient::new(openmeter_url, openmeter_token);

    for meter in meters {
        // Only process response_value meters
        if meter.metric != "response_value" {
            continue;
        }

        debug!("[PROXY] Response meter: {meter:?}");

        // If `json_path` does not exist, warn and skip
        let Some(json_path) = &meter.json_path else {
            warn!(
                "[Metrics] No json_path for response meter '{}', skipping.",
                meter.metric
            );
            continue;
        };

        // Extract value from JSON using the path
        let extracted_val = extract_json_path(json_response, json_path);
        debug!("[PROXY] Extracted response meter value: {extracted_val:?}");

        let Some(value) = extracted_val else {
            warn!(
                "[Metrics] Could not extract JSON for response meter '{}' using json_path='{}', skipping.",
                meter.metric,
                json_path
            );
            continue;
        };

        let data = serde_json::json!({
            "value": value,
            "metric": meter.metric,
            "container_id": container_id,
            "currency": meter.currency,
            "cost": meter.cost,
            "unit": meter.unit,
            "kind": "Container",
            "service": "Nebulous",
        });
        debug!("[PROXY] response metrics data: {data:?}");

        // Create CloudEvent
        let cloud_event = CloudEvent {
            id: ShortUuid::generate().to_string(),
            source: "nebulous-proxy".to_string(),
            specversion: "1.0".to_string(),
            r#type: meter.metric.clone(),
            subject: container_id.to_string(),
            time: Some(chrono::Utc::now().to_rfc3339()),
            dataschema: None,
            datacontenttype: Some("application/json".to_string()),
            data: Some(data),
        };

        // Send the event to OpenMeter
        if let Err(e) = meter_client.ingest_events(&[cloud_event]).await {
            error!(
                "[Metrics] Failed to report response meter {:?} for container {}: {}",
                meter, container_id, e
            );
            return Err(format!("Failed to send response metrics: {}", e));
        }

        debug!(
            "[Metrics] Successfully reported response meter {:?} for container {}",
            meter, container_id
        );
    }

    Ok(())
}
