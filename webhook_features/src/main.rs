mod errors;

use actix_web::{get, App, HttpServer};
use clap::Parser;

use crate::errors::{DrahtBotError, Result};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, help = "GitHub token")]
    token: String,
    #[arg(long, help = "Host to listen on", default_value = "0.0.0.0")]
    host: String,
    #[arg(long, help = "Port to listen on", default_value = "1337")]
    port: u16,
}

#[get("/")]
async fn index() -> &'static str {
    "Welcome to DrahtBot!"
}
#[actix_web::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    HttpServer::new(move || {
        App::new()
            .service(index)
    })
    .bind(format!("{}:{}", args.host, args.port))?
    .run()
    .await
    .map_err(|e| DrahtBotError::IOError(e))
}
