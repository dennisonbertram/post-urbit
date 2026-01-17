use std::path::Path;
use std::time::Instant;

use crate::admin_state::AdminState;
use crate::identity::IdentityManager;

pub async fn render_metrics(
    admin: &AdminState,
    identity: &IdentityManager,
    started_at: Instant,
) -> String {
    use std::fmt::Write;

    let iid = identity.iid().await;
    let uptime_seconds = started_at.elapsed().as_secs();
    let apps_installed = {
        let data = admin.data.lock().await;
        data.apps.len() as u64
    };
    let apps_running = 0u64;
    let identity_bytes = directory_size(&admin.data_dir.join("identity"));
    let messages_bytes = directory_size(&admin.data_dir.join("messages"));
    let sync_bytes = directory_size(&admin.data_dir.join("sync"));
    let apps_bytes = directory_size(&admin.data_dir.join("apps"));
    let runtime_bytes = directory_size(&admin.data_dir.join("runtime"));

    let mut out = String::new();
    let _ = writeln!(out, "postnode_uptime_seconds {}", uptime_seconds);
    let _ = writeln!(
        out,
        "postnode_info{{version=\"{}\", iid=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION"),
        iid
    );
    let _ = writeln!(out, "postnode_memory_bytes{{type=\"heap\"}} 0");
    let _ = writeln!(out, "postnode_memory_bytes{{type=\"resident\"}} 0");
    let _ = writeln!(out, "postnode_cpu_seconds_total 0");
    let _ = writeln!(out, "postnode_open_file_descriptors 0");
    let _ = writeln!(out, "postnode_connections_total{{type=\"direct\"}} 0");
    let _ = writeln!(out, "postnode_connections_total{{type=\"relay\"}} 0");
    let _ = writeln!(out, "postnode_connections_active 0");
    let _ = writeln!(out, "postnode_connection_events_total{{event=\"opened\"}} 0");
    let _ = writeln!(out, "postnode_connection_events_total{{event=\"closed\"}} 0");
    let _ = writeln!(out, "postnode_connection_events_total{{event=\"failed\"}} 0");
    let _ = writeln!(out, "postnode_bytes_sent_total 0");
    let _ = writeln!(out, "postnode_bytes_received_total 0");
    let _ = writeln!(out, "postnode_messages_sent_total{{type=\"direct\"}} 0");
    let _ = writeln!(out, "postnode_messages_sent_total{{type=\"group\"}} 0");
    let _ = writeln!(out, "postnode_messages_received_total{{type=\"direct\"}} 0");
    let _ = writeln!(out, "postnode_messages_received_total{{type=\"group\"}} 0");
    let _ = writeln!(out, "postnode_message_queue_depth{{queue=\"outgoing\"}} 0");
    let _ = writeln!(out, "postnode_message_queue_depth{{queue=\"incoming\"}} 0");
    let _ = writeln!(out, "postnode_apps_installed_total {}", apps_installed);
    let _ = writeln!(out, "postnode_apps_running {}", apps_running);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"identity\"}} {}", identity_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"messages\"}} {}", messages_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"sync\"}} {}", sync_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"apps\"}} {}", apps_bytes);
    let _ = writeln!(out, "postnode_storage_bytes{{database=\"runtime\"}} {}", runtime_bytes);
    out
}

fn directory_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total = total.saturating_add(meta.len());
                } else if meta.is_dir() {
                    total = total.saturating_add(directory_size(&entry.path()));
                }
            }
        }
    }
    total
}
