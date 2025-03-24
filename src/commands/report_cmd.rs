use notify::{watcher, RecursiveMode, Watcher};
use reqwest::blocking::Client;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::mpsc::channel;
use std::time::Duration;

fn main() {
    let file_path = "path/to/your/file.txt";
    let (tx, rx) = channel();

    // Create a watcher with a debounce interval
    let mut watcher = watcher(tx, Duration::from_secs(2)).unwrap();
    watcher
        .watch(file_path, RecursiveMode::NonRecursive)
        .unwrap();

    // This will track the last read position in the file.
    let mut last_read_offset = 0;
    let client = Client::new();

    loop {
        match rx.recv() {
            Ok(event) => {
                println!("Detected change: {:?}", event);

                // Open the file and get its current size.
                let mut file = File::open(file_path).expect("Unable to open file");
                let metadata = file.metadata().expect("Unable to get metadata");
                let file_size = metadata.len();

                // If the file has been truncated (size decreased), reset the offset.
                if file_size < last_read_offset {
                    last_read_offset = 0;
                }

                // Seek to the last known offset and read new data.
                file.seek(SeekFrom::Start(last_read_offset))
                    .expect("Seek failed");
                let mut new_data = String::new();
                file.read_to_string(&mut new_data)
                    .expect("Failed to read new data");

                // Update the offset for future changes.
                last_read_offset = file_size;

                // Only send if there's new data.
                if !new_data.is_empty() {
                    let response = client
                        .post("http://api.example.com/consume")
                        .body(new_data)
                        .send();
                    println!("API response: {:?}", response);
                }
            }
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
}
