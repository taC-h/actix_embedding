#![feature(prelude_import)]
#[prelude_import]
use std::prelude::v1::*;
#[macro_use]
extern crate std;
use actix_web::{web, App, HttpServer, Responder, HttpResponse};
async fn greet() -> impl Responder {
    HttpResponse::Ok().body("hello world")
}
fn run() -> std::io::Result<()> {
    actix_web::rt::System::new("run").block_on(async move {
        {
            HttpServer::new(|| App::new().route("/", web::get().to(greet)))
                .bind("127.0.0.1:8080")?
                .run()
                .await
        }
    })
}
fn main() {
    run().unwrap();
}
