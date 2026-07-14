use rusqlite::Connection;
use std::fs;

fn main() {
    let workspace_dir = std::path::PathBuf::from(r#"C:\Users\hongducdev\AppData\Roaming\Cursor\User\workspaceStorage"#);
    if let Ok(entries) = fs::read_dir(workspace_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let db_path = entry.path().join("state.vscdb");
            if db_path.exists() {
                if let Ok(conn) = Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
                    let mut stmt = conn.prepare("SELECT key, value FROM ItemTable WHERE key = 'composer.composerData' OR key = 'workbench.panel.aichat.view.aichat.chatdata'").unwrap();
                    let rows: Vec<(String, String)> = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().filter_map(|r| r.ok()).collect();
                    for (k, v) in rows {
                        println!("KEY: {}", k);
                        println!("LENGTH: {}", v.len());
                        println!("SAMPLE: {}...", &v.chars().take(200).collect::<String>());
                    }
                }
            }
        }
    }
}
