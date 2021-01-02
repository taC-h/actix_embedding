# actix-webのhttpサーバーをアプリケーションに埋め込む

# TL;DR
```rust
#[actix_web::main]
async fn run(
    addr: String,
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
    tx_start.send(Ok(())).unwrap();

    rx_stop.recv().unwrap();
    server.stop(true).await;
}
```
ソースはgithubに[あります](https://github.com/taC-h/actix_embedding)
`git clone https://github.com/taC-h/actix_embedding.git`

# なぜ
httpserverを埋め込みたい
だけど，actix-webのサンプルはどれも↓のように`#[actix_web::main]`やら`#[actix_web::rt]`やら書いてあるのでそのままでは移植できない

# マクロ展開する

```rust
use actix_web::{web, App, HttpServer, Responder, HttpResponse};

async fn greet() -> impl Responder {
    HttpResponse::Ok().body("hello world")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(greet))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

`actix_web::main`のソースは[こんな](https://docs.rs/actix-web-codegen/0.4.0/src/actix_web_codegen/lib.rs.html#172)感じ

```rust
#[proc_macro_attribute]
pub fn main(_: TokenStream, item: TokenStream) -> TokenStream {
    use quote::quote;

    let mut input = syn::parse_macro_input!(item as syn::ItemFn);
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &mut input.sig;
    let body = &input.block;
    let name = &sig.ident;

    if sig.asyncness.is_none() {
        return syn::Error::new_spanned(sig.fn_token, "only async fn is supported")
            .to_compile_error()
            .into();
    }

    sig.asyncness = None;

    (quote! {
        #(#attrs)*
        #vis #sig {
            actix_web::rt::System::new(stringify!(#name))
                .block_on(async move { #body })
        }
    })
    .into()
}
```
うーん，展開後のコードは普通の関数っぽい

確認のためにマクロ展開コマンド`cargo expand`を使う
インストールには
`cargo +nightly install cargo-expand`
で
rustcのマクロ展開オプションと違ってハイライトがつくので見やすい
ということで↓のコードを展開


```rust
use actix_web::{web, App, HttpServer, Responder, HttpResponse};

async fn greet() -> impl Responder {
    HttpResponse::Ok().body("hello world")
}

#[actix_web::main]
async fn run() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(greet))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

fn main() {
    run().unwrap();
}
```
展開後
```rust
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

```
大丈夫そう
別スレッドで実行すればいい感じ

```rust
use std::thread;
fn block(){
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s).unwrap();
}

fn main() {
    thread::spawn(|| run.unwrap(););
    block();
}
```


# mpscで開始，終了制御
さらに外部から開始のエラーハンドリングと，終了ができると良さげ
公式サンプルの [axtix/examples/shutdown-server](https://github.com/actix/examples/tree/master/shutdown-server) を見ながら変形


まず終了処理の実装から

```rust
use std::sync::mpsc;

#[actix_web::main]
async fn run(//引数の追加
    addr: String,
    rx: mpsc::Receiver<()>,
) -> std::io::Result<()> {
    let server = HttpServer::new(||
        App::new()
            .route("/", web::get().to(greet))
    ).bind(addr)?
    .run();
    //ここから追加分
    rx.recv().unwrap();
    server.stop(true).await;
    Ok(())
}

fn main() {
    let addr = "127.0.0.1:8080";
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        run(addr.to_owned(), tx_start, rx_stop)
    });

    println!("start server http://{}", addr);
    block();
    tx.send(()).unwrap();
    handle.join().unwrap().unwrap();

    println!("stopped");
    block();
}
```
サーバー自体を別スレッドで実行するのでシグナルの送信はメインスレッドから
サーバーアドレスもmoveして指定できるように
ちなみに`&str`で受けようとすると，`actix_web::main`のライフタイム境界エラーが出る

enterを押せば`tx`からシグナルを送信して`rx.recv()`のブロッキングが解除される仕組み
サーバー自体は`run()`した時点で稼働している

次に開始のエラーハンドリング

```rust
#[actix_web::main]
async fn run(
    addr: String,
    tx_start: mpsc::Sender<Result<(),String>>,
    rx_stop:mpsc::Receiver<()>,
) /*-> std::io::Result*/ {
    let server = HttpServer::new(||
        App::new()
            .route("/", web::get().to(greet))
    ).bind(addr);

    let server = match server {
        Ok(s) => s.run(),//構築成功
        Err(_) => {//構築失敗
            tx_start.send(Err("can't bind ip adrres".to_owned())).unwrap();
            return;
        }
    };
    tx_start.send(Ok(())).unwrap();//構築成功

    rx_stop.recv().unwrap();
    server.stop(true).await;
    //Ok(())
}

fn main() {
    let addr = "127.0.0.1:8080";
    let (tx_stop, rx_stop) = mpsc::channel();//リネーム
    let (tx_start, rx_start) = mpsc::channel();//追加
    let handle = thread::spawn(move || {
        run(addr.to_owned(), tx_start, rx_stop)
    });

    rx_start.recv().unwrap().unwrap_or_else(|e| {//サーバー構築失敗時処理
        eprintln!("error: {}",e);//標準エラー出力へ
        std::process::exit(1);
    });

    println!("start server http://{}", addr);
    block();

    tx_stop.send(()).unwrap();
    handle.join().unwrap();

    println!("stopped");
    block();
}
```


`?`を使わず`match`で処理することで`tx_start.send(/**/)`でエラーを送信
成功した場合は空の値を送信して`rx_start.recv()`のブロッキングのみ解除する仕組み

`run`の戻り値は`handle::join()`した時まで取り出せないから必要ない
`?`を消したので空にできる

# 最終コード
```rust
use actix_web::{web, App, HttpServer, Responder, HttpResponse};
use std::{
    sync::mpsc,
    thread,
};

fn block(){
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s).unwrap();
}

async fn greet() -> impl Responder {
    HttpResponse::Ok().body("hello world")
}

#[actix_web::main]
async fn run(
    addr: String,
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

    rx_start.recv().unwrap().unwrap_or_else(|e| {//サーバー構築失敗時処理
        eprintln!("error: {}",e);//標準エラー出力へ
        std::process::exit(1);
    });

    println!("start server http://{}", addr);
    block();

    tx_stop.send(()).unwrap();//stopシグナルを送信する
    handle.join().unwrap();//終了を待つ

    println!("stopped");
    block();//終了の確認用
}
```

# 参考資料
[actix_web::mainのソース](https://docs.rs/actix-web-codegen/0.4.0/src/actix_web_codegen/lib.rs.html#172)

[公式サンプル(シャットダウン編)](https://github.com/actix/examples/tree/master/shutdown-server)