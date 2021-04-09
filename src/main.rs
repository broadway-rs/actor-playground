use async_std::sync::Arc;
use async_std::task;
use futures::{future, FutureExt};
use switch_channel::async_channel::async_std::{diunbounded, DiSwitchReceiver, DiSwitchSender};
use switch_channel::err::recv::RecvError;
use async_trait::async_trait;
use std::ops::{Deref, DerefMut};
use async_std::sync::RwLock;

#[async_std::main]
async fn main() {
    println!("Hello, world!");
    let (mut sender, instance) = ActorInstance::<dyn Greeter>::start();
    let mut i: i128 = 0;
    loop{
        sender.greet().await;
        if i % 100 == 0{
            sender.set_name(i.to_string()).await;
        }
        i += 1;
        // This demo still works without this, but you'll see much longer stretches of names
        // WHICH IS FINE, it's just clearing out mutable calls and building up slow println calls.
        task::sleep(std::time::Duration::from_millis(1)).await;
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


#[async_trait]
pub trait Greeter{
    async fn set_name(&mut self, name: String);

    async fn greet(&self);
}

pub struct GreeterActor{
    name: String,
}

#[async_trait]
impl Greeter for GreeterActor{
    async fn set_name(&mut self, name: String){
        self.name = name;
    }

    async fn greet(&self){
        println!("Hello! My name is {}", self.name);
    }
}

impl Actor for GreeterActor{
    fn new() -> Self{
        GreeterActor{
            name: "James".to_string()
        }
    }
}

impl<'a> Role for dyn Greeter + 'a{
    type Actor = GreeterActor;

    type Calls = GreeterCalls;
    type MutCalls = GreeterMutCalls;
}

enum GreeterCalls{
    Greet,
}

unsafe impl Send for GreeterCalls{}

#[async_trait]
impl Handler<GreeterActor> for GreeterCalls{
    async fn handle(self, actor: &GreeterActor){
        match self{
            Self::Greet => GreeterActor::greet(actor).await,
        };
    }
}

enum GreeterMutCalls{
    SetName(String)
}

unsafe impl Send for GreeterMutCalls{}

#[async_trait]
impl MutHandler<GreeterActor> for GreeterMutCalls{
    async fn handle_mut(self, actor: &mut GreeterActor){
        match self{
            Self::SetName(name) => GreeterActor::set_name(actor, name).await,
        };
    }
}

#[async_trait]
impl Greeter for ActorChannel<dyn Greeter>{
    async fn set_name(&mut self, name: std::string::String) {
        self.mut_calls_sender.send(GreeterMutCalls::SetName(name)).await;
    }

    async fn greet(&self) { 
        self.calls_sender.send(GreeterCalls::Greet).await;
    }
}