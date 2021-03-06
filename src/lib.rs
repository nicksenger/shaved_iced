//! Helpers for managing asynchronous side-effects in Iced applications using stream composition.
//!
//! Provides an iced `Subscription` created with an operator function mapping an incoming `Stream` of
//! messages pushed to a `Sender` to an outgoing `Stream` of messages handle by the application's
//! `update` function.

mod stream;

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use iced_futures::futures::{self, channel, stream::BoxStream, StreamExt};

pub use stream::{Message, Sender, State};

/// Creates the shaved_iced subscription. You need to provide:
///
/// * `initial_state`: some state which will be made available from within the `operator`
/// * `update`: a function which may be used to update the provided `initial_state` in response
///to messages
/// * `operator`: a function which consumes the stream of messages from the `Sender` and returns
/// a new stream of messages to be fed back to the application
pub fn connect<T, U>(
    initial_state: U,
    update: impl Fn(&mut U, T) + Send + Sync + 'static,
    operator: impl FnOnce(BoxStream<'static, (Arc<T>, State<U>)>) -> BoxStream<'static, T> + 'static,
) -> iced_native::Subscription<Message<T>>
where
    U: Clone + 'static + Send + Sync,
    T: Clone + 'static + Send + Sync,
{
    iced_native::Subscription::from_recipe(Worker::new(initial_state, update, operator))
}

struct Worker<T, U>
where
    U: 'static + Send + Sync,
    T: 'static + Send,
{
    initial_state: U,
    update: Box<dyn Fn(&mut U, T) + Send + Sync>,
    operator: Box<dyn FnOnce(BoxStream<'static, (Arc<T>, State<U>)>) -> BoxStream<'static, T>>,
}

impl<T, U> Worker<T, U>
where
    U: 'static + Send + Sync,
    T: 'static + Send,
{
    pub fn new(
        initial_state: U,
        update: impl Fn(&mut U, T) + Send + Sync + 'static,
        operator: impl FnOnce(BoxStream<'static, (Arc<T>, State<U>)>) -> BoxStream<'static, T> + 'static,
    ) -> Self {
        Self {
            initial_state,
            update: Box::new(update),
            operator: Box::new(operator),
        }
    }
}

impl<H, I, T, U> iced_native::subscription::Recipe<H, I> for Worker<T, U>
where
    U: Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
    H: Hasher,
{
    type Output = Message<T>;

    fn hash(&self, state: &mut H) {
        struct Marker;
        std::any::TypeId::of::<Marker>().hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<'static, I>) -> BoxStream<'static, Self::Output> {
        let Self {
            initial_state,
            update,
            operator,
        } = *self;

        Sender::connect(initial_state, update, operator).1
    }
}

/// Combines 2 `operator`s into a single `operator`
pub fn combine_operators<T, U>(
    a: impl FnOnce(BoxStream<'static, (Arc<T>, State<U>)>) -> BoxStream<'static, T> + 'static,
    b: impl FnOnce(BoxStream<'static, (Arc<T>, State<U>)>) -> BoxStream<'static, T> + 'static,
) -> Box<dyn FnOnce(BoxStream<'static, (Arc<T>, State<U>)>) -> BoxStream<'static, T>>
where
    U: Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    use futures::SinkExt;

    Box::new(move |in_stream: BoxStream<'static, (Arc<T>, State<U>)>| {
        let (sender_a, receiver_a) = {
            let (sender, receiver) = channel::mpsc::unbounded();
            (sender, a(Box::pin(receiver)))
        };

        let (sender_b, receiver_b) = {
            let (sender, receiver) = channel::mpsc::unbounded();
            (sender, b(Box::pin(receiver)))
        };

        let sender = Box::pin(sender_a.fanout(sender_b));

        let a = in_stream.scan(sender, |sender, x| {
            let _ = sender.start_send_unpin(x);
            futures::future::ready(Some(None))
        });

        let b = Box::pin(futures::stream::select(receiver_a, receiver_b).map(Option::Some));

        Box::pin(futures::stream::select(a, b).filter_map(futures::future::ready))
    })
}

#[cfg(feature = "test")]
use thiserror::Error;

#[cfg(feature = "test")]
pub async fn test_connect<T, U>(
    initial_state: U,
    update: impl Fn(&mut U, T) + Send + Sync + 'static,
    operator: impl FnOnce(BoxStream<'static, (Arc<T>, State<U>)>) -> BoxStream<'static, T> + 'static,
) -> Result<(Sender<T>, BoxStream<'static, Message<T>>, State<U>), Error>
where
    U: Clone + 'static + Send + Sync,
    T: Clone + 'static + Send + Sync,
{
    let (state, mut receiver) = Sender::connect(initial_state, update, operator);

    if let Some(Message::Ready(sender)) = receiver.next().await {
        Ok((sender, receiver, state))
    } else {
        Err(Error::ConnectionFailed)
    }
}

#[cfg(feature = "test")]
#[derive(Error, Debug)]
pub enum Error {
    #[error("connection failed")]
    ConnectionFailed,
}
