use rusqlite::Connection;
fn main() {
    let path = std::path::PathBuf::from(r#"C:\Users\hongducdev\AppData\Roaming\Cursor\User\globalStorage\state.vscdb"#);
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let token: String = conn.query_row(
        "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
        [],
        |row| row.get(0),
    ).unwrap();
    
    let token_clean = token.trim_matches('\"');
    println!("TOKEN: {}...", &token_clean[0..15]);
    
    let client = reqwest::blocking::Client::new();
    let res = client.get("https://api2.cursor.sh/auth/stripe")
        .header("Authorization", format!("Bearer {}", token_clean))
        .send().unwrap();
    println!("API api2/auth/stripe: {} - {}", res.status(), res.text().unwrap());
    
    let res2 = client.get("https://api2.cursor.sh/user/details")
        .header("Authorization", format!("Bearer {}", token_clean))
        .send().unwrap();
    println!("API api2/user/details: {} - {}", res2.status(), res2.text().unwrap());
}
