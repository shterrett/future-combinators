#![warn(missing_docs)]
//! Combinators for Futures
//!
//! This provides a way to compose `Future`s before calling `.await`.

use core::pin::Pin;
use core::task::{Context, Poll};
use std::future::Future;

/// This is basically [`Result`], but less judgemental.
#[derive(Debug, Eq, PartialEq)]
pub enum Either<A, B> {
    /// One side
    Left(A),
    /// The other side
    Right(B),
}

/// Execute two futures and return the result of the future that finishes first. The other future
/// is dropped.
///
/// The result of `.await` is `Either<A, B>`
pub fn race<F, G, A, B>(a: F, b: G) -> Race<A, B>
where
    F: Future<Output = A>,
    F: 'static,
    G: Future<Output = B>,
    G: 'static,
{
    Race {
        a: Box::pin(a),
        b: Box::pin(b),
    }
}

/// Encapsulates racing futures
pub struct Race<A, B> {
    #[doc(hidden)]
    a: Pin<Box<dyn Future<Output = A>>>,
    #[doc(hidden)]
    b: Pin<Box<dyn Future<Output = B>>>,
}

impl<A, B> Future for Race<A, B> {
    type Output = Either<A, B>;

    fn poll(mut self: Pin<&mut Race<A, B>>, ctx: &mut Context) -> Poll<Either<A, B>> {
        match self.a.as_mut().poll(ctx) {
            Poll::Ready(r) => Poll::Ready(Either::Left(r)),
            Poll::Pending => match self.b.as_mut().poll(ctx) {
                Poll::Ready(r) => Poll::Ready(Either::Right(r)),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

/// Execute two futures and return the pair of their results. Will not return until both futures
/// are complete.
///
/// The result of `.await` is `(A, B)`
pub fn join<F, G, A, B>(a: F, b: G) -> Join<A, B>
where
    F: Future<Output = A>,
    F: 'static,
    G: Future<Output = B>,
    G: 'static,
{
    Join {
        a: Box::pin(a),
        b: Box::pin(b),
        r_a: Box::new(None),
        r_b: Box::new(None),
    }
}

/// Encapsulates the joining of futures.
pub struct Join<A, B> {
    a: Pin<Box<dyn Future<Output = A>>>,
    b: Pin<Box<dyn Future<Output = B>>>,
    r_a: Box<Option<A>>,
    r_b: Box<Option<B>>,
}

impl<A, B> Future for Join<A, B> {
    type Output = (A, B);
    fn poll(mut self: Pin<&mut Join<A, B>>, ctx: &mut Context) -> Poll<(A, B)> {
        let mut m_a = self.r_a.take();
        let mut m_b = self.r_b.take();
        if m_a.is_none() {
            match self.a.as_mut().poll(ctx) {
                Poll::Ready(r) => {
                    m_a = Some(r);
                }
                Poll::Pending => (),
            }
        };
        if m_b.is_none() {
            match self.b.as_mut().poll(ctx) {
                Poll::Ready(r) => {
                    m_b = Some(r);
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

/// Executes a future that returns a result. If if returns `Err`, then the second future is
/// executed. If it returns `Ok`, the second future is never run.
///
/// The result of `.await` is `(Option<E>, A)`.
pub fn on_error<F, G, A, E>(f: F, e: G) -> OnError<A, E>
where
    F: Future<Output = Result<A, E>>,
    F: 'static,
    G: Future<Output = A>,
    G: 'static,
{
    OnError {
        f: Box::pin(f),
        e: Box::pin(e),
        err: Box::new(None),
    }
}

/// Encapsulates a future and an option on-error.
pub struct OnError<A, E> {
    f: Pin<Box<dyn Future<Output = Result<A, E>>>>,
    e: Pin<Box<dyn Future<Output = A>>>,
    err: Box<Option<E>>,
}

impl<A, E> Future for OnError<A, E> {
    type Output = (Option<E>, A);
    fn poll(mut self: Pin<&mut OnError<A, E>>, ctx: &mut Context) -> Poll<(Option<E>, A)> {
        if self.err.is_none() {
            match self.f.as_mut().poll(ctx) {
                Poll::Ready(Ok(a)) => Poll::Ready((None, a)),
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

#[cfg(test)]
mod test {
    extern crate async_std;
    use super::*;
    use async_std::task;
    use std::time::Duration;

    async fn timed_return(ms: u64) -> u64 {
        task::sleep(Duration::from_millis(ms)).await;
        ms
    }

    #[async_std::test]
    async fn test_race_left() {
        let f_1 = timed_return(1);
        let f_2 = timed_return(2);
        let r = race(f_1, f_2);
        let result = r.await;
        assert_eq!(result, Either::Left(1));
    }

    #[async_std::test]
    async fn test_race_right() {
        let f_1 = timed_return(1);
        let f_2 = timed_return(2);
        let r = race(f_2, f_1);
        let result = r.await;
        assert_eq!(result, Either::Right(1));
    }
    #[async_std::test]
    async fn test_join() {
        let f_1 = timed_return(1);
        let f_2 = timed_return(2);
        let j = join(f_1, f_2);
        let result = j.await;
        assert_eq!(result, (1, 2));
    }

    async fn successful() -> Result<usize, usize> {
        Ok(1)
    }
    async fn failure() -> Result<usize, usize> {
        Err(3)
    }
    async fn on_err() -> usize {
        2
    }

    #[async_std::test]
    async fn test_on_err_successful() {
        let on_e = on_error(successful(), on_err());
        let result = on_e.await;
        assert_eq!(result, (None, 1));
    }

    #[async_std::test]
    async fn test_on_err_failure() {
        let on_e = on_error(failure(), on_err());
        let result = on_e.await;
        assert_eq!(result, (Some(3), 2));
    }
}
