use etcdv3_rs::etcd_actions;
use etcdv3_rs::etcd_proto::*;
use futures;
use futures::stream::Stream;
use tokio_core;

enum FutureResponse {
    Watch(String, WatchResponse),
    ChannelReceive(WatcherCommands),
    Error,
}

enum WatcherCommands {
    Subscribe(String, Box<FnMut(Vec<WatchTrigger>) + Send>),
}

pub struct Watcher {
    core: tokio_core::reactor::Core,
    receiver: futures::sync::mpsc::UnboundedReceiver<WatcherCommands>,
    etcd_session: etcd_actions::EtcdSession,
}

#[derive(Clone)]
pub struct WatcherHandle {
    sender: futures::sync::mpsc::UnboundedSender<WatcherCommands>,
}

// Convert `etcd_proto::WatchResponse` into something that is easier to use.
#[derive(Default, Clone)]
pub struct WatchTrigger {
    pub key: String,
    /// `None` if there is no key -- this is the case during deletions (but maybe also for other events)?
    pub value: Option<Vec<u8>>,
    /// Does this event represent a deletion?
    pub deleted: bool,
}

impl WatchTrigger {
    pub fn from_event(event: &Event) -> WatchTrigger {
        let deleted = *(event.event_type().as_ref().unwrap_or(&EventType::PUT)) == EventType::DELETE;
        // FIXME: This assumes that a key-value is always included with an event. I (@apanda) do not know why this would
        // not be the case but need to confirm that this is in fact the case.
        let kv = event.kv.as_ref().unwrap();
        WatchTrigger {
            deleted: deleted,
            ..WatchTrigger::from_kv(kv)
        }
    }

    pub fn from_kv(kv: &KeyValue) -> WatchTrigger {
        let key = kv.key().unwrap().clone();
        let value = kv.value_as_u8().map(|v| v.clone());
        WatchTrigger {
            key: key,
            value: value,
            ..Default::default()
        }
    }
}

impl WatcherHandle {
    fn new(sender: futures::sync::mpsc::UnboundedSender<WatcherCommands>) -> WatcherHandle {
        WatcherHandle { sender: sender }
    }

    pub fn watch_pfx<F: FnMut(Vec<WatchTrigger>) + Send + 'static>(&self, key: &str, func: F) {
        self.sender.send(WatcherCommands::Subscribe(String::from(key), Box::new(func))).unwrap();
    }
}

impl Watcher {
    pub fn new(etcd_url: &str) -> (Watcher, WatcherHandle) {
        let (sender, receiver) = futures::sync::mpsc::unbounded(); // Limit the number of outstanding messages.
        let core = tokio_core::reactor::Core::new().unwrap();
        let session = etcd_actions::EtcdSession::new(&core.handle(), etcd_url);
        (Watcher {
            core:  core,
            receiver: receiver,
            etcd_session: session
        }, WatcherHandle::new(sender))
    }

    // This function does not return.
    pub fn run(mut self) -> ! {
        let receiver = self.receiver
                           .map(|response| FutureResponse::ChannelReceive(response))
                           .map_err(|_| FutureResponse::Error);
        let mut stream : Box<Stream<Error=FutureResponse, Item=FutureResponse>> = Box::new(receiver);
        loop {
            let (what, s) = match self.core.run(stream.into_future()) {
                Ok((response, s)) => (response, s),
                Err((e, s)) => {
                    eprintln!("Received error in control thread event processing");
                    (Some(e), s)
                }
            };
            stream = match what {
                Some(msg) => {
                    match msg {
                        FutureResponse::Watch(_, _) => {
                            // Don't currently need to do anything here, but leaving this in for future.
                            s
                        },
                        FutureResponse::ChannelReceive(cmd) => {
                            match cmd {
                                WatcherCommands::Subscribe(keypfx, mut f) => {
                                    // We synchronously set up the watch since this prevents us from having
                                    // to covert a future to a stream (that will only run once).
                                    // FIXME: Profile to see if this really makes any difference.
                                    let pfx_copy = keypfx.clone();
                                    let watch = self.core.run(self.etcd_session.watch_pfx(&keypfx))
                                        .unwrap()
                                        .map(move |response| {
                                            let watch_triggers:Vec<_> = response.events.as_ref()
                                                .map(|ue| {
                                                    ue.iter()
                                                .map(|ev| WatchTrigger::from_event(ev))
                                                .collect() })
                                                .unwrap_or_else(|| vec![]);
                                            f(watch_triggers);
                                            FutureResponse::Watch(pfx_copy.clone(), response)
                                    })
                                    .map_err(|_| FutureResponse::Error);
                                    Box::new(s.select(watch))
                                }
                            }
                        },
                        FutureResponse::Error => {
                            s
                        }
                    }
                },
                None => s
            };
        }
    }
}
