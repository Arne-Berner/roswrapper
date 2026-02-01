mod messages;
use messages::*;
mod show_result;
use std::{iter, sync::mpsc::{self, Receiver, Sender}};

use winit::
    event_loop::EventLoop
;

// Bring in traits we need from roslibrust
use roslibrust::{Publish, codegen::Time, traits::{Ros, Subscribe}};

use crate::{messages::{geometry_msgs::{Point, Pose, Quaternion}, nav_msgs::MapMetaData}, show_result::{App, BufferBundle, DataBundle, UserEvent, random_buffer_bundle}};
use wgpu::{Device, Queue, util::{BufferInitDescriptor, DeviceExt}};
use bke_ccl::CCLState;

// Writing a simple publisher behavior using roslibrust's generic traits
async fn pub_counter(ros: impl Ros) {
    // This will nicely control the rat our code runs at
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    // Create a publisher on our topic
    let publisher = ros
        .advertise::<nav_msgs::OccupancyGrid>("/map")
        .await
        .expect("Could not create publisher!");

    loop {
        // Wait for next tick of our interval timer
        interval.tick().await;

        // Lock our state and read the current value
        // TODO Präsentation
        let data_bundle = random_buffer_bundle();
        // let data_bundle = DataBundle{ data: vec![1, -1, -1, 0], width: 4, height: 1};

        // Publish the current value
        publisher
            .publish(&nav_msgs::OccupancyGrid {
                header: std_msgs::Header {
                    stamp: Time {
                        secs: 23,
                        nsecs: 400,
                    },
                    frame_id: "abc".to_string(),
                },
                info: MapMetaData {
                    map_load_time: Time {
                        secs: 23,
                        nsecs: 400,
                    },
                    resolution: 1.0,
                    width: data_bundle.width,
                    height: data_bundle.height,
                    origin: Pose {
                        position: Point {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        orientation: Quaternion {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                            w: 0.0,
                        },
                    },
                },
                data: data_bundle.data,
            })
            .await
            .expect("Failed to publish message!");

    }
}


async fn sub_counter(ros: impl Ros, sender: Sender<BufferBundle>, receiver: Receiver<(Device, Queue)>) {
    // Create a subscriber on our topic
    let mut subscriber = ros
        .subscribe::<nav_msgs::OccupancyGrid>("/map")
        .await
        .expect("Could not create subscriber!");

    let (device, queue): (Device, Queue) = receiver.recv().unwrap();
    loop {
        // Wait for next message
        let msg = subscriber.next().await.expect("Failed to get message!");
        let height = msg.info.height;
        let width = msg.info.width;

        // Print the message
        
        let image_bytes: Vec<u32> = msg.data.iter().copied().map(|x| {
            // (!x.max(0) & 1) as u32
            if x == -1 {
                1 as u32
            } else {
                (x.max(0) & 1) as u32
            }
        }).collect();
        let input_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("input"),
            contents: bytemuck::cast_slice(&image_bytes),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        });
        // it's fine for now that it repeats the action.
        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        // TODO I need to change BKE so that it works like compute_visualizer with the bind groups
        let ccl = CCLState::new(
            &device,
            &queue,
            &input_buffer,
            msg.info.width,
            msg.info.height,
        ).unwrap();
        let buffer = ccl.compute(&mut encoder).unwrap();
        queue.submit(iter::once(encoder.finish()));
        let buffer_bundle = BufferBundle{buffer, height, width};
        sender.send(buffer_bundle).unwrap();
    }
}

#[tokio::main]
async fn main() {
    // Initialize a logger to help with debugging
    env_logger::init();

    // Create a rosbridge client we can use
    let ros = roslibrust::rosbridge::ClientHandle::new("ws://localhost:9090")
        .await
        .expect("Failed to connect to rosbridge!");

    let (buffer_tx, buffer_rx) = mpsc::channel::<BufferBundle>();
    let (device_tx, device_rx) = mpsc::channel::<(Device,Queue)>();

    // Spawn a new tokio task to run our subscriber:
    // tokio::spawn(pub_counter(ros.clone()));
    tokio::spawn(sub_counter(ros.clone(), buffer_tx.clone(), device_rx));


    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let mut app = App::new();
    tokio::spawn(async move 
        {
            let mut set_state = false;
            loop {
                if set_state == false {
                    proxy.send_event(UserEvent::CreateDevice(device_tx.clone())).unwrap();
                    set_state = true;
                } else {
                    let buffer_bundle = buffer_rx.recv().unwrap();
                    proxy.send_event(UserEvent::UpdateBuffer(buffer_bundle)).unwrap();
                }
            }
        }
    );
    event_loop.run_app(&mut app).unwrap();
    // Wait for ctrl_c
    tokio::signal::ctrl_c().await.unwrap();
}
