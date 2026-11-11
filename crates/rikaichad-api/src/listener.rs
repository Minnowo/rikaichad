// SPDX-License-Identifier: AGPL-3.0-or-later
//! A TCP listener for `axum::serve` that caps open connections and closes
//! connections with no I/O progress for a while.
//!
//! `axum::serve` sets no hyper timer, so hyper's header-read timeout never
//! applies and connections are unlimited. With `--bind` on a network, a
//! client that opens many connections and sends nothing (slow loris) would
//! otherwise hold them forever. A client trickling one byte just under the
//! idle timeout can still hold a slot; the connection cap bounds that.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, Sleep};

/// Pause after a failed `accept` (e.g. out of file descriptors) before
/// trying again, so the loop does not spin.
const ACCEPT_RETRY_PAUSE: Duration = Duration::from_millis(50);

/// Listener with a connection cap and per-connection idle timeout.
pub struct GuardedListener {
    inner: TcpListener,
    permits: Arc<Semaphore>,
    idle_timeout: Duration,
}

impl GuardedListener {
    /// Wraps `inner`: at most `max_connections` open at once (further
    /// clients wait in the OS backlog), each closed after `idle_timeout`
    /// without reads or writes.
    pub fn new(inner: TcpListener, max_connections: usize, idle_timeout: Duration) -> Self {
        GuardedListener {
            inner,
            permits: Arc::new(Semaphore::new(max_connections)),
            idle_timeout,
        }
    }
}

impl axum::serve::Listener for GuardedListener {
    type Io = IdleTimeoutStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        // The semaphore is never closed, so acquiring cannot fail; if it
        // somehow did, stop accepting rather than exceed the cap.
        let Ok(permit) = Arc::clone(&self.permits).acquire_owned().await else {
            return std::future::pending().await;
        };
        loop {
            match self.inner.accept().await {
                Ok((stream, addr)) => {
                    return (
                        IdleTimeoutStream::new(stream, permit, self.idle_timeout),
                        addr,
                    );
                }
                Err(_) => tokio::time::sleep(ACCEPT_RETRY_PAUSE).await,
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

/// A connection that fails its pending read or write once `idle` passes
/// without progress, and frees its connection slot when dropped.
pub struct IdleTimeoutStream {
    stream: TcpStream,
    timer: Pin<Box<Sleep>>,
    idle: Duration,
    _permit: OwnedSemaphorePermit,
}

impl IdleTimeoutStream {
    fn new(stream: TcpStream, permit: OwnedSemaphorePermit, idle: Duration) -> Self {
        IdleTimeoutStream {
            stream,
            timer: Box::pin(tokio::time::sleep(idle)),
            idle,
            _permit: permit,
        }
    }

    fn progressed(&mut self) {
        self.timer.as_mut().reset(Instant::now() + self.idle);
    }

    /// Turns a pending I/O poll into a timeout error once the timer fires.
    fn check_idle<T>(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<T>> {
        match self.timer.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "connection idle for too long",
            ))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncRead for IdleTimeoutStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Pin::new(&mut this.stream).poll_read(cx, buf) {
            Poll::Ready(result) => {
                this.progressed();
                Poll::Ready(result)
            }
            Poll::Pending => this.check_idle(cx),
        }
    }
}

impl AsyncWrite for IdleTimeoutStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.stream).poll_write(cx, data) {
            Poll::Ready(result) => {
                this.progressed();
                Poll::Ready(result)
            }
            Poll::Pending => this.check_idle(cx),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}
