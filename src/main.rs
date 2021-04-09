use std::sync::RwLock;
use switch_channel::async_channel::async_std::{diunbounded, DiSwitchReceiver, DiSwitchSender};

fn main() {
    println!("Hello, world!");
}

trait Role{
    type Actor: Actor;

    type Calls;
    type MutCalls;
}

trait Actor{
    fn new() -> Self;

    fn start(&self);

    fn stop(&self);
}

struct ActorInstance<T: Role>{
    actor: RwLock<T::Actor>,
    calls: DiSwitchReceiver<T::Calls>,
    mut_calls: DiSwitchReceiver<T::MutCalls>,
}

struct ActorChannel<T: Role>{
    calls_sender: DiSwitchSender<T::Calls>,
    mut_calls_sender: DiSwitchSender<T::MutCalls>,
}

impl<T: Role> ActorInstance<T>{
    fn new() -> (ActorInstance<T>, ActorChannel<T>){
        let (mut_calls_sender, mut_calls) = diunbounded();
        let (calls_sender, calls) = diunbounded();

        (ActorInstance{
            actor: RwLock::new(T::Actor::new()),
            calls,
            mut_calls
        },
        ActorChannel{
            calls_sender,
            mut_calls_sender
        })
    }
}