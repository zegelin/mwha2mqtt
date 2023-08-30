use std::time::Duration;

use anyhow::Result;
use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Packet};
use tokio::{task, time};

#[tokio::main]
async fn main() -> Result<()> {

    let mut mqttoptions = MqttOptions::new("rumqtt-async", "localhost", 1883);
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    task::spawn(async move {
        while let Ok(notification) = eventloop.poll().await {
            match notification {
                Event::Incoming(Packet::Publish(publish)) => {

                },
                _ => {}
            }
        }
    });

    


    client.subscribe("hello/rumqtt", QoS::AtMostOnce).await.unwrap();

    task::spawn(async move {
        for i in 0..10 {
            client.publish("hello/rumqtt", QoS::AtLeastOnce, false, vec![i; i as usize]).await.unwrap();
            time::sleep(Duration::from_millis(100)).await;
        }
    });

    

    Ok(())
}