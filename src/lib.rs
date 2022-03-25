use core::pin::Pin;
use core::task::{Context, Poll};
use std::future::Future;

enum Either<A, B> {
    Left(A),
    Right(B),
}

struct Race<A, B> {
    a: Pin<Box<dyn Future<Output = A> + Unpin>>,
    b: Pin<Box<dyn Future<Output = B> + Unpin>>,
}

impl<A, B> Future for Race<A, B> {
    type Output = Either<A, B>;

    fn poll(mut self: Pin<&mut Race<A, B>>, ctx: &mut Context) -> Poll<Either<A, B>> {
        match self.a.as_mut().poll(ctx) {
            Poll::Ready(r) => Poll::Ready(Either::Left(r)), // need to cancel `b`
            Poll::Pending => match self.b.as_mut().poll(ctx) {
                Poll::Ready(r) => Poll::Ready(Either::Right(r)), // need to cancel `a`
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

struct Join<A, B> {
    a: Pin<Box<dyn Future<Output = A> + Unpin>>,
    b: Pin<Box<dyn Future<Output = B> + Unpin>>,
    r_a: Box<Option<A>>,
    r_b: Box<Option<B>>,
}

impl<A, B> Future for Join<A, B> {
    type Output = (A, B);
    fn poll(mut self: Pin<&mut Join<A, B>>, ctx: &mut Context) -> Poll<(A, B)> {
        let mut m_a = self.r_a.take();
        let mut m_b = self.r_b.take();
        if m_b.is_none() {
            match self.a.as_mut().poll(ctx) {
                Poll::Ready(r) => {
                    m_a = Some(r);
                    ()
                }
                Poll::Pending => (),
            }
        };
        if m_b.is_none() {
            match self.b.as_mut().poll(ctx) {
                Poll::Ready(r) => {
                    m_b = Some(r);
                    ()
                }
                Poll::Pending => (),
            }
        };
        match (m_a, m_b) {
            (Some(a), Some(b)) => Poll::Ready((a, b)),
            (m_aa, m_bb) => {
                self.r_a = Box::new(m_aa);
                self.r_b = Box::new(m_bb);
                Poll::Pending
            }
        }
    }
}

struct OnError<A, E> {
    f: Pin<Box<dyn Future<Output = Result<A, E>> + Unpin>>,
    e: Pin<Box<dyn Future<Output = A> + Unpin>>,
    err: Box<Option<E>>,
}

impl<A, E> Future for OnError<A, E> {
    type Output = (Option<E>, A);
    fn poll(mut self: Pin<&mut OnError<A, E>>, ctx: &mut Context) -> Poll<(Option<E>, A)> {
        if self.err.is_none() {
            match self.f.as_mut().poll(ctx) {
                Poll::Ready(Ok(a)) => Poll::Ready((None, a)), // need to cancel `e`
                Poll::Ready(Err(e)) => {
                    self.err = Box::new(Some(e));
                    match self.e.as_mut().poll(ctx) {
                        Poll::Pending => Poll::Pending,
                        Poll::Ready(a) => Poll::Ready((self.err.take(), a)),
                    }
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            match self.e.as_mut().poll(ctx) {
                Poll::Ready(a) => Poll::Ready((self.err.take(), a)),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}
