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
pub fn race<F, G, A, B>(a: F, b: G) -> Race<F, A, G, B>
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
pub struct Race<F, A, G, B>
where
    F: Future<Output = A>,
    G: Future<Output = B>,
{
    #[doc(hidden)]
    a: Pin<Box<F>>,
    #[doc(hidden)]
    b: Pin<Box<G>>,
}

impl<F, A, G, B> Future for Race<F, A, G, B>
where
    F: Future<Output = A>,
    G: Future<Output = B>,
{
    type Output = Either<A, B>;

    fn poll(mut self: Pin<&mut Race<F, A, G, B>>, ctx: &mut Context) -> Poll<Either<A, B>> {
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
pub fn join<F, A, G, B>(a: F, b: G) -> Join<F, A, G, B>
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
pub struct Join<F, A, G, B>
where
    F: Future<Output = A>,
    G: Future<Output = B>,
{
    a: Pin<Box<F>>,
    b: Pin<Box<G>>,
    r_a: Box<Option<A>>,
    r_b: Box<Option<B>>,
}

impl<F, A, G, B> Future for Join<F, A, G, B>
where
    F: Future<Output = A>,
    G: Future<Output = B>,
{
    type Output = (A, B);
    fn poll(mut self: Pin<&mut Join<F, A, G, B>>, ctx: &mut Context) -> Poll<(A, B)> {
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
pub fn on_error<F, A, G, E>(f: F, e: G) -> OnError<F, A, G, E>
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
pub struct OnError<F, A, G, E>
where
    F: Future<Output = Result<A, E>>,
    G: Future<Output = A>,
{
    f: Pin<Box<F>>,
    e: Pin<Box<G>>,
    err: Box<Option<E>>,
}

impl<F, A, G, E> Future for OnError<F, A, G, E>
where
    F: Future<Output = Result<A, E>>,
    G: Future<Output = A>,
{
    type Output = (Option<E>, A);
    fn poll(mut self: Pin<&mut OnError<F, A, G, E>>, ctx: &mut Context) -> Poll<(Option<E>, A)> {
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

/// Maps a function over the result of a future
///
/// Given a `Future<A>` and a `Fn<A> -> B`, the result of `.await` is `B`.
pub fn map<F, A, B, G>(future: F, f: G) -> Map<F, A, B, G>
where
    F: Future<Output = A>,
    F: 'static,
    G: Fn(A) -> B,
{
    Map {
        future: Box::pin(future),
        f: Box::new(f),
    }
}

/// Encapsulates mapping over a future
pub struct Map<F, A, B, G>
where
    F: Future<Output = A>,
    G: Fn(A) -> B,
{
    future: Pin<Box<F>>,
    f: Box<G>,
}

impl<F, A, B, G> Future for Map<F, A, B, G>
where
    F: Future<Output = A>,
    G: Fn(A) -> B,
{
    type Output = B;
    fn poll(mut self: Pin<&mut Map<F, A, B, G>>, ctx: &mut Context) -> Poll<B> {
        self.future.as_mut().poll(ctx).map(self.f.as_ref())
    }
}

/// Runs a function that returns a future over the result of a future
/// i.e. `future >>= function`
///
/// For `Future<Output = A>` and a `Fn(A) -> Future<Output = B>`,
/// the result of calling `.await` is `B`
pub fn and_then<F, A, B, G, H>(fut: F, f: G) -> AndThen<F, A, B, G, H>
where
    F: Future<Output = A>,
    G: Fn(A) -> H,
    H: Future<Output = B>,
{
    AndThen {
        future: Box::pin(fut),
        f: Box::new(f),
        and_then: None,
    }
}

/// Encapsulates a monadic bind for futures
pub struct AndThen<F, A, B, G, H>
where
    F: Future<Output = A>,
    G: Fn(A) -> H,
    H: Future<Output = B>,
{
    future: Pin<Box<F>>,
    f: Box<G>,
    and_then: Option<Pin<Box<H>>>,
}

impl<F, A, B, G, H> Future for AndThen<F, A, B, G, H>
where
    F: Future<Output = A>,
    G: Fn(A) -> H,
    H: Future<Output = B>,
    H: 'static,
{
    type Output = B;
    fn poll(mut self: Pin<&mut AndThen<F, A, B, G, H>>, ctx: &mut Context) -> Poll<B> {
        match self.and_then.take() {
            Some(mut at) => at.as_mut().poll(ctx),
            None => match self.future.as_mut().poll(ctx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(a) => {
                    let f_b = Box::pin((self.f)(a));
                    self.and_then.replace(f_b);
                    self.and_then.as_mut().unwrap().as_mut().poll(ctx)
                }
            },
        }
    }
}

/// Traverse a vector of futures.
///
/// Given `Vec<Future<Output = A>>`, calling `.await` returns `Vec<A>`
/// Order and length of the original vector is preserved.
pub fn traverse_vec<F, A>(fs: Vec<F>) -> TraverseV<F, A>
where
    F: Future<Output = A>,
    F: 'static,
{
    TraverseV {
        futures: fs
            .into_iter()
            .map(|f| (Box::pin(f), Box::new(None)))
            .collect(),
    }
}

/// Encapsulates traversing a vector of futures
pub struct TraverseV<F, A>
where
    F: Future<Output = A>,
{
    futures: Vec<(Pin<Box<F>>, Box<Option<A>>)>,
}

impl<F, A> Future for TraverseV<F, A>
where
    F: Future<Output = A>,
{
    type Output = Vec<A>;
    fn poll(mut self: Pin<&mut TraverseV<F, A>>, ctx: &mut Context) -> Poll<Vec<A>> {
        for (future, result) in self.as_mut().futures.iter_mut() {
            match result.as_mut() {
                None => {
                    if let Poll::Ready(a) = future.as_mut().poll(ctx) {
                        result.replace(a);
                    }
                }
                Some(_) => {}
            }
        }
        let done = self.futures.iter().all(|(_, o)| o.is_some());
        if done {
            Poll::Ready(
                self.as_mut()
                    .futures
                    .iter_mut()
                    // checked that all are `Some` in `done`
                    .map(|(_, o)| o.take().unwrap())
                    .collect(),
            )
        } else {
            Poll::Pending
        }
    }
}

/// Traverse a `Result` containing a `Future` in `Ok`
///
/// Given a `Result<Future<Output = A>, E>`, calling `.await` produces `Result<A, E>`
pub fn traverse_result<F, A, E>(r: Result<F, E>) -> TraverseR<F, A, E>
where
    F: Future<Output = A>,
{
    match r {
        Err(e) => TraverseR {
            future: None,
            error: Box::new(Some(e)),
        },
        Ok(f) => TraverseR {
            future: Some(Box::pin(f)),
            error: Box::new(None),
        },
    }
}

/// Encapsulates traversing a `Future` over a `Result`
pub struct TraverseR<F, A, E>
where
    F: Future<Output = A>,
{
    future: Option<Pin<Box<F>>>,
    error: Box<Option<E>>,
}
impl<F, A, E> Future for TraverseR<F, A, E>
where
    F: Future<Output = A>,
{
    type Output = Result<A, E>;
    fn poll(mut self: Pin<&mut TraverseR<F, A, E>>, ctx: &mut Context) -> Poll<Result<A, E>> {
        // Using two options and `is_some` are uncomfortable contortions to work around the fact
        // that in the `Err` case, the value needs to be moved out and returned, but in the `Ok`
        // case, the future needs to be mutably borrowed and `poll`-ed.
        match (self.error.is_some(), self.future.is_some()) {
            (true, false) => Poll::Ready(Err(self.error.take().unwrap())),
            (false, true) => self.future.as_mut().unwrap().as_mut().poll(ctx).map(Ok),
            _ => panic!("Invariant violated: TraverseR should  have exactly one Some"),
        }
    }
}

/// Traverse a `Future` over an `Option`
///
/// Given `Option<Future<Output = A>>`, the result of `.await` is `Option<A>`
pub fn traverse_option<F, A>(o: Option<F>) -> TraverseO<F, A>
where
    F: Future<Output = A>,
{
    TraverseO {
        future: o.map(Box::pin),
    }
}

/// Encapsulates traversing a `Future` over `Option`
pub struct TraverseO<F, A>
where
    F: Future<Output = A>,
{
    future: Option<Pin<Box<F>>>,
}

impl<F, A> Future for TraverseO<F, A>
where
    F: Future<Output = A>,
{
    type Output = Option<A>;
    fn poll(mut self: Pin<&mut TraverseO<F, A>>, ctx: &mut Context) -> Poll<Option<A>> {
        match self.future.as_mut() {
            None => Poll::Ready(None),
            Some(f) => f.as_mut().poll(ctx).map(Some),
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

    #[async_std::test]
    async fn test_map() {
        let f = map(on_err(), |a| a + 1);
        let result = f.await;
        assert_eq!(result, 3);
    }

    async fn from_result(r: Result<usize, usize>) -> usize {
        match r {
            Ok(x) => x,
            Err(y) => y,
        }
    }

    #[async_std::test]
    async fn test_and_then() {
        let at = and_then(successful(), from_result);
        let result = at.await;
        assert_eq!(result, 1);
    }

    #[async_std::test]
    async fn test_traverse_vec() {
        let vals = vec![1, 3, 2, 4];
        let fs = vals.clone().into_iter().map(timed_return).collect();
        let f = traverse_vec(fs);
        let results = f.await;
        assert_eq!(results, vals);
    }

    #[async_std::test]
    async fn test_traverse_result_ok() {
        let r: Result<u64, u64> = Ok(1);
        let f = traverse_result(r.map(timed_return));
        let x = f.await;
        assert_eq!(x, Ok(1));
    }

    #[async_std::test]
    async fn test_traverse_result_err() {
        let r: Result<u64, u64> = Err(1);
        let f = traverse_result(r.map(timed_return));
        let x = f.await;
        assert_eq!(x, Err(1));
    }

    #[async_std::test]
    async fn test_traverse_option_some() {
        let r: Option<u64> = Some(1);
        let f = traverse_option(r.map(timed_return));
        let x = f.await;
        assert_eq!(x, Some(1));
    }

    #[async_std::test]
    async fn test_traverse_option_none() {
        let r: Option<u64> = None;
        let f = traverse_option(r.map(timed_return));
        let x = f.await;
        assert_eq!(x, None);
    }
}
