use std::sync::Arc;

use iced_futures::futures::{
    self,
    channel::mpsc,
    lock::{Mutex, MutexGuard},
    StreamExt,
};

#[derive(Debug, Clone)]
pub enum Message<T>
where
    T: Send + Sync + 'static,
{
    /// Indicates that the sender is ready to receive messages
    Ready(Sender<T>),
    /// Provides an update to the application
    Update(T),
}

/// State available to the asynchronous side of shaved iced
#[derive(Clone)]
pub struct State<U> {
    data: Arc<Mutex<U>>,
}

impl<U> State<U> {
    pub fn new(data: U) -> Self {
        Self {
            data: Arc::new(Mutex::new(data)),
        }
    }

    /// Read the state using the provided function
    pub async fn get<T>(&self, f: impl Fn(&U) -> T) -> T {
        let data = self.data.lock().await;
        f(&(*data))
    }

    async fn lock<'a>(&'a mut self) -> MutexGuard<'a, U> {
        self.data.lock().await
    }
}

/// Sender for sending messages into shaved iced
#[derive(Clone)]
pub struct Sender<T> {
    sender: mpsc::UnboundedSender<T>,
}

impl<T> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender").finish()
    }
}

impl<T> Sender<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn send(&mut self, message: T) {
        let _ = self.sender.start_send(message);
    }
}

impl<T> Sender<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub(crate) fn connect<U>(
        initial_state: U,
        update: impl Fn(&mut U, T) + 'static + Send + Sync,
        operator: impl FnOnce(
                futures::stream::BoxStream<'static, (Arc<T>, State<U>)>,
            ) -> futures::stream::BoxStream<'static, T>
            + 'static,
    ) -> futures::stream::BoxStream<'static, Message<T>>
    where
        U: Clone + Send + Sync + 'static,
    {
        let (sender, receiver) = mpsc::unbounded();

        let mut sender = Self {
            sender: sender.clone(),
        };
        Box::pin(
            futures::stream::unfold(Some(sender.clone()), move |state| async move {
                if let Some(sender) = state {
                    Some((Message::Ready(sender), None))
                } else {
                    None
                }
            })
            .chain(
                operator(Box::pin(receiver.scan(
                    (State::new(initial_state), Arc::new(update)),
                    |(state, update), message| {
                        let mut state = state.clone();
                        let update = update.clone();
                        async move {
                            {
                                let mut data = state.lock().await;
                                update(&mut (*data), message.clone());
                            }
                            Some((Arc::new(message), state.clone()))
                        }
                    },
                )))
                .map(move |message| {
                    sender.send(message.clone());
                    Message::Update(message)
                }),
            ),
        )
    }
}
