use crate::{errors::handler::HandlerError, models::ws::MutexWebSocketClientWriter};
use axum::extract::ws::Message;
use futures_util::SinkExt;
use std::collections::VecDeque;

// [task3] pcm data processing
pub async fn pcm_data_processing(
    window_size: u64,
    slide_size: u64,
    mut pcm_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    shared_client_writer: MutexWebSocketClientWriter,
) -> Result<(), HandlerError> {
    let mut counter: u64 = 0;
    let mut stock_buffer: VecDeque<Vec<u8>> = VecDeque::new();
    let mut send_buffer: Vec<u8> = Vec::new();

    //* step6: receive binary from sender (producer) *//
    //? Receiver (Consumer) //
    while let Some(bin) = pcm_rx.recv().await {
        //* collect buffer *//
        stock_buffer.push_back(bin);
        counter += 1;

        tracing::info!("counter: {}", counter);

        //* step7: do sliding window *//
        if counter >= window_size {
            // スライド幅分だけバッファを準備
            for _ in 0..slide_size {
                if let Some(buf) = stock_buffer.pop_front() {
                    send_buffer.extend(buf);
                }
            }

            // TODO: ここで時間のかかる解析処理を実行する
            // let events = analyze(&send_buffer);
            // let packet = VJDataPacket { pcm_data: send_buffer.clone(), events };
            // let serialized_packet = rmp_serde::to_vec(&packet).unwrap();

            //* step8: send binary data to client with window size *//
            let mut writer = shared_client_writer.lock().await;
            writer
                .send(Message::Binary(send_buffer.clone().into()))
                .await?;

            // reset counter
            counter -= slide_size;
            // reset send buffer
            send_buffer.clear();
        }
    }
    Ok(())
}
