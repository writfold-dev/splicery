pub fn connect(url: &str) -> Result<Conn, Error> {
    let conn = open(url)?;
    conn.handshake()?;
    Ok(conn)
}

pub fn probe(url: &str) -> bool {
    open(url).is_ok()
}
