use axum::body::BodyDataStream;
use bytes::{Bytes, BytesMut};
use futures_util::TryStreamExt;

pub async fn read_all_data_fold(body_stream: BodyDataStream) -> anyhow::Result<Bytes> {
    let buffer = body_stream
        .try_fold(BytesMut::new(), |mut acc, chunk| async move {
            acc.extend_from_slice(&chunk);
            Ok(acc)
        })
        .await?;

    Ok(buffer.freeze())
}
