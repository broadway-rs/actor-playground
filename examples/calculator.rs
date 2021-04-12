use async_std::task;
use async_trait::async_trait;
use async_std::channel::{unbounded};
use broadway::actor::*;
use broadway::broadway_macro::role;

#[async_std::main]
async fn main(){
    println!("Hello, world!");
    let (mut sender, instance) = ActorInstance::<dyn Calculator>::start();
    println!("{}", sender.add(10).await);
    sender.sub(5).await;
    println!("{}", sender.get().await);
}

#[role(String, CalculatorActor)]
#[async_trait]
pub trait Calculator{
    async fn get(&self) -> isize;

    async fn set(&mut self, value: isize) -> isize;

    async fn add(&mut self, amount: isize) -> isize;
    
    async fn sub(&mut self, amount: isize) -> isize;

    async fn mul(&mut self, amount: isize) -> isize;

    async fn div(&mut self, amount: isize) -> isize;
}

pub struct CalculatorActor{
    current: isize,
}

#[async_trait]
impl Calculator for CalculatorActor{
    async fn get(&self) -> isize{
        self.current
    }

    async fn set(&mut self, value: isize) -> isize{
        self.current = value;
        self.current
    }

    async fn add(&mut self, amount: isize) -> isize{
        self.current += amount;
        self.current
    }

    async fn sub(&mut self, amount: isize) -> isize{
        self.current -= amount;
        self.current
    }

    async fn mul(&mut self, amount: isize) -> isize{
        self.current *= amount;
        self.current
    }

    async fn div(&mut self, amount: isize) -> isize{
        self.current /= amount;
        self.current
    }
}

impl Actor for CalculatorActor{
    fn new() -> Self { 
        Self{
            current: 0
        }
    }
}