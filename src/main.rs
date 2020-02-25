// TODO: set up lint denys

use crate::{error::PointercrateError, middleware::headers::Headers, model::user::UserPagination, state::PointercrateState};
use actix_web::{middleware::Logger, web::scope, App, HttpServer};
use api::{
    auth,
    demonlist::{demon, misc, player, record, submitter},
    user,
};
use std::net::SocketAddr;

#[macro_use]
mod util;
mod api;
mod cistring;
mod config;
mod documentation;
mod error;
mod extractor;
mod middleware;
mod model;
mod permissions;
mod ratelimit;
mod state;
mod video;

pub type Result<T> = std::result::Result<T, PointercrateError>;

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    dotenv::dotenv().expect("Failed to initialize .env file!");

    let application_state = PointercrateState::initialize().await;

    // TODO: error handler
    // TODO: json config
    // TODO: 404 and 405 handling

    HttpServer::new(move || {
        App::new()
            .wrap(Headers)
            .wrap(Logger::default())
            .app_data(application_state.clone())
            .service(
                scope("/api/v1")
                    .service(misc::list_information)
                    .service(
                        scope("/auth")
                            .service(auth::register)
                            .service(auth::delete_me)
                            .service(auth::get_me)
                            .service(auth::invalidate)
                            .service(auth::login)
                            .service(auth::patch_me),
                    )
                    .service(
                        scope("/users")
                            .service(user::paginate)
                            .service(user::get)
                            .service(user::delete)
                            .service(user::patch),
                    )
                    .service(
                        scope("/submitters")
                            .service(submitter::get)
                            .service(submitter::paginate)
                            .service(submitter::patch),
                    )
                    .service(
                        scope("/demons")
                            .service(demon::v1::get)
                            .service(demon::v1::paginate)
                            .service(demon::v1::patch)
                            .service(demon::v1::delete_creator)
                            .service(demon::v1::post_creator)
                            .service(demon::post),
                    )
                    .service(
                        scope("/records")
                            .service(record::delete)
                            .service(record::get)
                            .service(record::paginate)
                            .service(record::patch)
                            .service(record::submit),
                    )
                    .service(
                        scope("/players")
                            .service(player::patch)
                            .service(player::paginate)
                            .service(player::get)
                            .service(player::ranking),
                    ),
            )
            .service(
                scope("/api/v2").service(
                    scope("/demons")
                        .service(demon::v2::paginate_listed)
                        .service(demon::v2::get)
                        .service(demon::v2::paginate)
                        .service(demon::v2::patch)
                        .service(demon::v2::delete_creator)
                        .service(demon::v2::post_creator)
                        .service(demon::post),
                ),
            )
    })
    .bind(SocketAddr::from(([127, 0, 0, 1], config::port())))?
    .run()
    .await?;

    Ok(())
}
