mod errors;
mod features;

use std::str::FromStr;

use crate::features::summary_comment::SummaryCommentFeature;
use actix_web::{get, post, web, App, HttpRequest, HttpServer, Responder};
use clap::Parser;
use features::Feature;
use octocrab::Octocrab;
use strum::{Display, EnumString};

use crate::errors::{DrahtBotError, Result};

#[derive(Parser, Debug)]
#[command(about="Run features on webhooks", long_about = None)]
struct Args {
    #[arg(short, long, help = "GitHub token")]
    token: String,
    #[arg(long, help = "Host to listen on", default_value = "0.0.0.0")]
    host: String,
    #[arg(long, help = "Port to listen on", default_value = "1337")]
    port: u16,
    #[arg(long, help = "Enable debug mode")]
    debug: bool,
}

#[derive(Debug, Display, EnumString, PartialEq, Eq, Clone, Copy)]
#[strum(serialize_all = "snake_case")]
pub enum GitHubEvent {
    Create,
    IssueComment,
    Ping,
    PullRequest,
    PullRequestReview,
    PullRequestReviewComment,
    Push,

    Unknown,
}

#[get("/")]
async fn index() -> &'static str {
    "Welcome to DrahtBot!"
}
#[derive(Debug, Clone)]
pub struct Context {
    octocrab: Octocrab,
    bot_username: String,
    debug: bool,
}

#[post("/postreceive")]
async fn postreceive_handler(
    ctx: web::Data<Context>,
    req: HttpRequest,
    data: web::Json<serde_json::Value>,
) -> impl Responder {
    let event_str = req
        .headers()
        .get("X-GitHub-Event")
        .unwrap()
        .to_str()
        .unwrap();
    let event = GitHubEvent::from_str(event_str).unwrap_or(GitHubEvent::Unknown);

    emit_event(&ctx, event, data).await.unwrap();

    "OK"
}

fn features() -> Vec<Box<dyn Feature>> {
    vec![Box::new(SummaryCommentFeature::new())]
}

async fn emit_event(
    ctx: &Context,
    event: GitHubEvent,
    data: web::Json<serde_json::Value>,
) -> Result<()> {
    for feature in features() {
        if feature.meta().events().contains(&event) {
            feature.handle(ctx, event, &data).await?;
        }
    }

    Ok(())
}

#[actix_web::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let octocrab = octocrab::Octocrab::builder()
        .personal_token(args.token)
        .build()
        .map_err(DrahtBotError::GitHubError)?;

    println!("DrahtBot will will run the following features:");
    for feature in features() {
        println!(" - {}", feature.meta().name());
        println!("   {}", feature.meta().description());
    }

    println!();

    // Get the bot's username
    let bot_username = octocrab
        .current()
        .user()
        .await
        .map_err(DrahtBotError::GitHubError)?
        .login;

    println!("Running as {}...", bot_username);

    let context = Context {
        octocrab,
        bot_username,
        debug: args.debug,
    };

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(context.clone()))
            .service(index)
            .service(postreceive_handler)
    })
    .bind(format!("{}:{}", args.host, args.port))?
    .run()
    .await
    .map_err(DrahtBotError::IOError)
}
