use actix_cors::Cors;
use actix_web::{get, rt, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_ws::AggregatedMessage;
use ollama_rs::{
    generation::{
        chat::{request::ChatMessageRequest, ChatMessage, MessageRole},
        completion::request::GenerationRequest,
    },
    Ollama,
};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

#[get("/ws")]
async fn ws(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let (res, mut session, stream) = actix_ws::handle(&req, stream)?;
    let client = Ollama::default();

    let mut stream = stream
        .aggregate_continuations()
        // aggregate continuation frames up to 1MiB
        .max_continuation_size(2_usize.pow(20));

    // start task but don't wait for it
    rt::spawn(async move {
        // receive messages from websocket
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(AggregatedMessage::Text(text)) => {
                    // echo text message
                    let messages = serde_json::from_str::<Vec<ChatMessage>>(&text).unwrap();
                    let mut chat_stream = client
                        .send_chat_messages_stream(ChatMessageRequest::new(
                            "llama3:latest".to_string(),
                            messages.clone(),
                        ))
                        .await
                        .unwrap();

                    while let Some(res) = chat_stream.next().await {
                        let message = serde_json::to_string(&res.unwrap()).unwrap();
                        session.text(message).await.unwrap();
                    }
                }

                Ok(AggregatedMessage::Binary(bin)) => {
                    // echo binary message
                    session.binary(bin).await.unwrap();
                }

                Ok(AggregatedMessage::Ping(msg)) => {
                    // respond to PING frame with PONG frame
                    session.pong(&msg).await.unwrap();
                }

                _ => {}
            }
        }
    });

    // respond immediately with response connected to WS session
    Ok(res)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let client = Ollama::default();

    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allow_any_header()
                    .allow_any_method()
                    .allow_any_origin(),
            )
            .service(ws)
    })
    .bind(("0.0.0.0", 4000))?
    .run()
    .await
}
