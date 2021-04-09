use async_std::sync::Arc;
use async_std::task;
use futures::{future, FutureExt};
use switch_channel::async_channel::async_std::{diunbounded, DiSwitchReceiver, DiSwitchSender};
use switch_channel::err::recv::RecvError;
use async_trait::async_trait;
use std::ops::{Deref, DerefMut};
use async_std::sync::RwLock;
use async_std::channel::{Sender, Receiver, unbounded};

#[async_std::main]
async fn main() {
    println!("Hello, world!");
    let (sender, instance) = ActorInstance::<dyn Greeter>::start();
    let mut i: i128 = 0;
    loop{
        let mut sender = sender.clone();
        if i % 100 == 0{
            task::spawn(async move {sender.set_name(i.to_string()).await});
        }
        else{
            task::spawn(async move {println!("{}", sender.greet().await)});
        }
        i += 1;
        // This demo still works without this, but you'll see much longer stretches of names
        // WHICH IS FINE, it's just clearing out mutable calls and building up slow println calls.
        //task::sleep(std::time::Duration::from_millis(1)).await;
    }
}

trait Role{
    type Actor: Actor;

    type Calls: Handler<Self::Actor>;
    type MutCalls: MutHandler<Self::Actor>;
}

trait Actor: Send + Sync{
    fn new() -> Self;

    fn start(&self){}

    fn stop(&self){}
}

struct ActorInstance<T: Role + ?Sized>{
    handling_loop: Option<Box<dyn std::future::Future<Output = T::Actor>>>,
}

struct ActorChannel<T: Role + ?Sized>{
    calls_sender: DiSwitchSender<T::Calls>,
    mut_calls_sender: DiSwitchSender<T::MutCalls>,
}

impl<T: Role + ?Sized> Clone for ActorChannel<T>{
    fn clone(&self) -> Self { 
        Self{
            calls_sender: self.calls_sender.clone(),
            mut_calls_sender: self.mut_calls_sender.clone(),
        }    
    }
}

impl<T: 'static + Role + ?Sized> ActorInstance<T>{
    fn start() -> (ActorChannel<T>, ActorInstance<T>){
        let (mut_calls_sender, mut_calls) = diunbounded();
        let (calls_sender, calls) = diunbounded();

        let actor = T::Actor::new();
        actor.start();
        
        (ActorChannel{
            calls_sender,
            mut_calls_sender
        },
        ActorInstance{
            handling_loop: Some(Box::new(task::spawn(Self::run_actor(RwLock::new(actor), calls, mut_calls))))
        })
    }

    async fn run_actor(
        actor: RwLock<T::Actor>,
        calls: DiSwitchReceiver<T::Calls>, 
        mut_calls: DiSwitchReceiver<T::MutCalls>) -> T::Actor{
        actor.read().await.start();
        let mut call_fut = None;
        let mut mut_call_fut = None;
        let mut priority = false;
        let actor = Arc::new(actor);
        loop{
            if let None = call_fut{
                if !calls.is_empty() || !calls.is_closed(){
                    call_fut = Some(Box::pin(calls.recv()));
                }
            }
            if let None = mut_call_fut {
                if !mut_calls.is_empty() || !mut_calls.is_closed(){
                    mut_call_fut = Some(Box::pin(mut_calls.recv()));
                }
            }

            match (call_fut.take(), mut_call_fut.take()){
                (Some(call), Some(mut_call)) =>{
                    // Not the biggest fan of this setup, but select
                    // favors the left side so we need to swap to prevent
                    // starvation
                    if priority{
                        match future::select(call, mut_call).await{
                            future::Either::Left((call, mut_call)) => {
                                // Call Logic
                                Self::call_loop(actor.clone(), call, &calls).await;
                                mut_call_fut = Some(mut_call);
                            },
                            future::Either::Right((mut_call, call)) => {
                                // Immut call logic
                                Self::mut_call_loop(actor.clone(), mut_call, &mut_calls).await;
                                call_fut = Some(call);
                            },
                        }
                    }
                    else{
                        match future::select(mut_call, call).await{
                            future::Either::Right((call, mut_call)) => {
                                // Call Logic
                                Self::call_loop(actor.clone(), call, &calls).await;
                                mut_call_fut = Some(mut_call);
                            },
                            future::Either::Left((mut_call, call)) => {
                                // Immut call logic
                                Self::mut_call_loop(actor.clone(), mut_call, &mut_calls).await;
                                call_fut = Some(call);
                            },
                        }
                    }
                    priority = !priority;
                },
                (Some(call_fut), None) => Self::call_loop(actor.clone(), call_fut.await, &calls).await,
                (None, Some(mut_call_fut)) => Self::mut_call_loop(actor.clone(), mut_call_fut.await, &mut_calls).await,
                (None, None) => break,
            };
        }
        Arc::try_unwrap(actor).ok().unwrap().into_inner()
    }

    async fn call_loop(actor: Arc<RwLock<T::Actor>>, first: Result<T::Calls, RecvError>, calls: &DiSwitchReceiver<T::Calls>){
        future::join_all(
            first.ok().into_iter()
            .chain(calls.switch().into_iter())
            .map(|call|{
                let actor = actor.clone();
                task::spawn(async move {call.handle(&actor.read().await.deref()).await})
            }))
            .await;
    }

    async fn mut_call_loop(actor: Arc<RwLock<T::Actor>>, first: Result<T::MutCalls, RecvError>, calls: &DiSwitchReceiver<T::MutCalls>){
        for call in first.ok().into_iter().chain(calls.switch().into_iter()){
            call.handle_mut(&mut actor.write().await.deref_mut()).await
        };
    }
}

unsafe impl<T: Role> Send for ActorInstance<T>{}
unsafe impl<T: Role> Sync for ActorInstance<T>{}

#[async_trait]
trait Handler<T>: Send{
    async fn handle(self, actor: &T);
}

#[async_trait]
trait MutHandler<T>: Send{
    async fn handle_mut(self, actor: &mut T);
}

struct Call<C, R>{
    return_channel: Sender<R>,
    call: C
}

unsafe impl<T: Send, R: Send> Send for Call<T, R>{}

// EVERYTHING FROM HERE TO THE NEXT COMMENT IS PRETTY MUCH
// ALL AN END USER SHOULD NEED TO WRITE FOR AN ACTOR, EVERYTHING ELSE
// SHOULD/COULD BE A LIBRARY ITEM OR MACRO GENERATED
#[async_trait]
pub trait Greeter{
    async fn set_name(&mut self, name: String);

    async fn greet(&self) -> String;
}

pub struct GreeterActor{
    name: String,
}

#[async_trait]
impl Greeter for GreeterActor{
    async fn set_name(&mut self, name: String){
        self.name = name;
    }

    async fn greet(&self) -> String{
        format!("Hello! My name is {}", self.name)
    }
}

impl Actor for GreeterActor{
    fn new() -> Self{
        GreeterActor{
            name: "James".to_string()
        }
    }
}

// IDEALLY THE END USER WOULD HAVE A MACRO THAT LOOKS LIKE THIS:
// #[role(GreeterActor)]
// That they would put over 'pub trait Greeter'
// that would generate everything below this comment.
impl<'a> Role for dyn Greeter + 'a{
    type Actor = GreeterActor;

    type Calls = Call<GreeterCall, GreeterReply>;
    type MutCalls = Call<GreeterMutCall, GreeterReply>;
}

enum GreeterReply{
    Greet(String),
    SetName(()),
}

enum GreeterCall{
    Greet,
}


#[async_trait]
impl Handler<GreeterActor> for Call<GreeterCall, GreeterReply>{
    async fn handle(self, actor: &GreeterActor){
        match self.call{
            GreeterCall::Greet => self.return_channel.send(GreeterReply::Greet(GreeterActor::greet(actor).await)).await,
        };
    }
}

enum GreeterMutCall{
    SetName(String)
}

#[async_trait]
impl MutHandler<GreeterActor> for Call<GreeterMutCall, GreeterReply>{
    async fn handle_mut(self, actor: &mut GreeterActor){
        match self.call{
            GreeterMutCall::SetName(name) => self.return_channel.send(GreeterReply::SetName(GreeterActor::set_name(actor, name).await)).await,
        };
    }
}

#[async_trait]
impl Greeter for ActorChannel<dyn Greeter>{
    async fn set_name(&mut self, name: std::string::String) {
        let (return_channel, receiver) = unbounded();
        self.mut_calls_sender.send(
            Call{
                return_channel,
                call: GreeterMutCall::SetName(name)
            }
        ).await;
        receiver.recv().await.unwrap();
    }

    async fn greet(&self) -> String { 
        let (return_channel, receiver) = unbounded();
        self.calls_sender.send(
            Call{
                return_channel,
                call: GreeterCall::Greet
            }
        ).await;
        if let Ok(GreeterReply::Greet(greeting)) = receiver.recv().await{
            greeting
        }
        else{
            panic!("Ruh roh");
        }
    }
}