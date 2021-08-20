use std::sync::Arc;

use iced::futures::StreamExt;
use iced::{
    button, Align, Application, Button, Clipboard, Column, Command, Element, Settings,
    Subscription, Text,
};
use shaved_iced::{self, combine_operators, Sender};
use tokio::time::sleep;

pub fn main() -> iced::Result {
    Counter::run(Settings::default())
}

#[derive(Default)]
struct Counter {
    pending_hits: Vec<Side>,
    sender: Option<Sender<Side>>,
    pings: i32,
    pongs: i32,
    ping_button: button::State,
    pong_button: button::State,
}

#[derive(Debug, Clone)]
enum Message {
    Hit(Side), // Message for when the user clicks "ping" or "pong"
    ShavedIced(shaved_iced::Message<Side>), // Messages coming asynchronously from ShavedIced
}

#[derive(Debug, Clone)]
enum Side {
    Ping,
    Pong,
}

impl Application for Counter {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Flags = ();

    fn subscription(&self) -> Subscription<Self::Message> {
        shaved_iced::connect( // Wiring up the application to use shaved iced
            // If we want some state available for our async logic, we can specify it here
            (),
            // If we want to update said state in response to messages coming _out_ of shaved iced, we can do so here:
            |_, _| {}, 
            // Operators are used map the stream of messages coming in to messages going out. The `combine_operators`
            // utility is provided so that you can compose operators
            combine_operators(vec![Box::new(ping_operator), Box::new(pong_operator)]),
        )
        .map(Message::ShavedIced)
    }

    fn update(&mut self, message: Message, _clipboard: &mut Clipboard) -> Command<Message> {
        match message {
            Message::Hit(hit) => {
                match hit {
                    Side::Ping => {
                        self.pings += 1;
                    }
                    Side::Pong => {
                        self.pongs += 1;
                    }
                }

                if let Some(sender) = &mut self.sender {
                    sender.send(hit)
                } else {
                    self.pending_hits.push(hit);
                }
            }
            // When the sender is ready we store it in our application state and send it the queued
            // messages that we've accumulated
            Message::ShavedIced(shaved_iced::Message::Ready(mut sender)) => {
                self.pending_hits.drain(..).for_each(|m| sender.send(m));
                self.sender = Some(sender)
            }
            Message::ShavedIced(shaved_iced::Message::Update(hit)) => match hit {
                // Respond to messages coming back out of shaved iced
                Side::Ping => {
                    self.pings += 1;
                }
                Side::Pong => {
                    self.pongs += 1;
                }
            },
        }

        Command::none()
    }

    fn view(&mut self) -> Element<Message> {
        Column::new()
            .padding(20)
            .align_items(Align::Center)
            .push(
                Button::new(&mut self.ping_button, Text::new("Ping"))
                    .on_press(Message::Hit(Side::Ping)),
            )
            .push(Text::new(format!("Pings: {}", self.pings.to_string())).size(50))
            .push(Text::new(format!("Pongs: {}", self.pongs.to_string())).size(50))
            .push(
                Button::new(&mut self.pong_button, Text::new("Pong"))
                    .on_press(Message::Hit(Side::Pong)),
            )
            .into()
    }

    fn new(_flags: ()) -> (Counter, Command<Message>) {
        (
            Counter {
                pending_hits: vec![],
                sender: None,
                pings: 0,
                pongs: 0,
                ping_button: button::State::new(),
                pong_button: button::State::new(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "Ping Pong - Shaved Iced".to_owned()
    }
}

// This operator handles how we will response to pings (be sending a pong 2 seconds later)
fn ping_operator(
    in_stream: iced::futures::stream::BoxStream<'static, (Arc<Side>, shaved_iced::State<()>)>,
) -> iced::futures::stream::BoxStream<'static, Side> {
    Box::pin(
        in_stream
            .filter_map(|(side, _)| async move {
                match *side {
                    Side::Ping => Some(async {
                        sleep(std::time::Duration::from_secs(2)).await;
                        Some(Side::Pong)
                    }),
                    Side::Pong => None,
                }
            })
            .buffer_unordered(100)
            .filter_map(iced::futures::future::ready),
    )
}

// This operator handles how we will response to pongs (by sending a ping 2 seconds later)
fn pong_operator(
    in_stream: iced::futures::stream::BoxStream<'static, (Arc<Side>, shaved_iced::State<()>)>,
) -> iced::futures::stream::BoxStream<'static, Side> {
    Box::pin(
        in_stream
            .filter_map(|(side, _)| async move {
                match *side {
                    Side::Pong => Some(async {
                        sleep(std::time::Duration::from_secs(2)).await;
                        Some(Side::Ping)
                    }),
                    Side::Ping => None,
                }
            })
            .buffer_unordered(100)
            .filter_map(iced::futures::future::ready),
    )
}
