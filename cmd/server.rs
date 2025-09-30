use pingora::server::Server;

fn main() -> anyhow::Result<()> {
    let mut my_server = Server::new(None)?;
    my_server.bootstrap();

    println!("socket tunnel proxy started on 0.0.0.0:8080");
    println!("Waiting for agent connections via WebSocket...");
    my_server.run_forever();
}
