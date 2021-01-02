use actix_web::{web, App, HttpServer, Responder, HttpResponse};
use std::{
    sync::mpsc,
    thread,
};

fn block(){//ブロックできれば何でもよい
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s).unwrap();
}

async fn greet() -> impl Responder {
    HttpResponse::Ok().body("hello world")
}

#[actix_web::main]
async fn run(
    addr: String,//&strにするとライフタイム境界に引っかかる
    tx_start: mpsc::Sender<Result<(),String>>,
    rx_stop:mpsc::Receiver<()>,
) {
    let server = HttpServer::new(||
        App::new()
            .route("/", web::get().to(greet))
    ).bind(addr);

    let server = match server {
        Ok(s) => s.run(),
        Err(_) => {
            tx_start.send(Err("can't bind ip adrres".to_owned())).unwrap();
            return;
        }
    };
    tx_start.send(Ok(())).unwrap();//start成功シグナルを送信

    rx_stop.recv().unwrap();//stopシグナルを受信
    server.stop(true).await;//終了を待つ
}

fn main() {
    let addr = "127.0.0.1:8080";
    let (tx_stop, rx_stop) = mpsc::channel();//サーバーstopシグナル用
    let (tx_start, rx_start) = mpsc::channel();//同start成功シグナル用
    let handle = thread::spawn(move || {
        run(addr.to_owned(), tx_start, rx_stop)
    });

    rx_start.recv().unwrap().unwrap_or_else(|e| {//サーバー構築に失敗したら
        eprintln!("error: {}",e);
        std::process::exit(1);
    });

    println!("start server http://{}", addr);
    block();

    tx_stop.send(()).unwrap();//stopシグナルを送信する
    handle.join().unwrap();//終了を待つ

    println!("stopped");
    block();//終了の確認用
}