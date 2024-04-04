use std::pin::Pin;
use std::task::{ Context, Poll };
use tokio::sync::mpsc;
use bytes::Bytes;
use hyper::body::{ Body, SizeHint, Frame };


pub struct Sender(mpsc::Sender<Bytes>);

pub struct ResponseBody {
    size: Option<u64>,
    recv: mpsc::Receiver<Bytes>
}

impl Sender {
    pub async fn send_data(&mut self, data: Bytes) -> Result<(), mpsc::error::SendError<()>> {
        self.0.send(data)
            .await
            .map_err(|_| mpsc::error::SendError(()))
    }
}

impl ResponseBody {
    pub fn empty() -> ResponseBody {
        let (_tx, rx) = mpsc::channel(1);
        ResponseBody {
            size: Some(0),
            recv: rx
        }
    }

    pub fn one(buf: Bytes) -> ResponseBody {
        let (tx, rx) = mpsc::channel(1);
        let size = buf.len() as u64;
        tokio::spawn(async move {
            let _ = tx.send(buf).await;
        });
        ResponseBody {
            size: Some(size),
            recv: rx
        }
    }

    pub fn channel(size: Option<u64>) -> (Sender, ResponseBody) {
        let (tx, rx) = mpsc::channel(32);
        (Sender(tx), ResponseBody { size, recv: rx })
    }
}

impl Body for ResponseBody {
    type Data = Bytes;
    type Error = !;

    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>>
    {
        let this = self.get_mut();

        match this.recv.poll_recv(cx) {
            Poll::Ready(Some(buf)) => {
                if let Some(size) = this.size.as_mut() {
                    *size -= buf.len() as u64;
                }

                Poll::Ready(Some(Ok(Frame::data(buf))))
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending
        }
    }

    fn size_hint(&self) -> SizeHint {
        let mut hint = SizeHint::new();
        if let Some(size) = self.size {
            hint.set_exact(size);
        }
        hint
    }
}
