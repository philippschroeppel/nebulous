use redis::streams::{StreamInfoGroupsReply, StreamPendingReply};
use redis::{Commands, Connection, RedisResult};

// This is our custom struct, not from redis::streams
#[derive(Debug, Clone)]
pub struct StreamProgress {
    total_entries: u64,
    pending_entries: u64,
    undelivered_entries: u64,
}

impl StreamProgress {
    pub fn remaining_entries(&self) -> u64 {
        self.pending_entries + self.undelivered_entries
    }

    pub fn progress_percentage(&self) -> f64 {
        if self.total_entries == 0 {
            return 100.0;
        }

        let processed = self.total_entries - self.remaining_entries();
        (processed as f64 / self.total_entries as f64) * 100.0
    }
}

pub fn get_consumer_group_progress(
    con: &mut Connection,
    stream_key: &str,
    group_name: &str,
) -> RedisResult<StreamProgress> {
    // 1. Get total stream entries using XLEN
    let total_entries: u64 = con.xlen(stream_key)?;

    // 2. Get pending entries (delivered but not acknowledged) using XPENDING
    let pending_info: StreamPendingReply = con.xpending(stream_key, group_name)?;
    let pending_entries = match &pending_info {
        StreamPendingReply::Data(data) => data.count,
        StreamPendingReply::Empty => 0,
    };

    // 3. Get consumer group info to find last-delivered-id
    let groups_info: StreamInfoGroupsReply = con.xinfo_groups(stream_key)?;
    let mut last_delivered_id = "0-0".to_string();

    for group in &groups_info.groups {
        if group.name == group_name {
            last_delivered_id = group.last_delivered_id.clone();
            break;
        }
    }

    // 4. Get count of entries after last-delivered-id (undelivered) using generic command
    let undelivered_entries: u64 = redis::cmd("XCOUNT")
        .arg(stream_key)
        .arg(format!("({}", last_delivered_id))
        .arg("+")
        .query(con)?;

    Ok(StreamProgress {
        total_entries,
        pending_entries: pending_entries as u64,
        undelivered_entries,
    })
}

// fn main() -> redis::RedisResult<()> {
//     let client = Client::open("redis://127.0.0.1/")?;
//     let mut con = client.get_connection()?;

//     let stream_key = "my_stream";
//     let group_name = "my_consumer_group";

//     match get_consumer_group_progress(&mut con, stream_key, group_name) {
//         Ok(progress) => {
//             println!("Stream: {} - Consumer Group: {}", stream_key, group_name);
//             println!("Total entries in stream: {}", progress.total_entries);
//             println!(
//                 "Pending entries (delivered but not acked): {}",
//                 progress.pending_entries
//             );
//             println!("Undelivered entries: {}", progress.undelivered_entries);
//             println!(
//                 "Total remaining to process: {}",
//                 progress.remaining_entries()
//             );
//             println!("Progress: {:.2}% complete", progress.progress_percentage());
//         }
//         Err(e) => eprintln!("Error: {}", e),
//     }

//     Ok(())
// }
