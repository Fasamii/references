use std::{ffi::OsStr, path::PathBuf};

use udev::{Event, EventType, MonitorBuilder};

fn main() {
    // list_block_devices();
    // monitor_usb();
    // udev_with_drm_kms();
    // monitor_usb_wait();
    // query_drm_output_devices();
    // listen_for_hotplugged_monitors();
    listen_for_hotplugged_monitors_plug_unplug();
}

fn list_block_devices() {
    let mut enumerator = udev::Enumerator::new().unwrap();
    // filters only to block subsystem devices
    enumerator.match_subsystem("block").unwrap(); // <- bolck means block devices (disks)

    for device in enumerator.scan_devices().unwrap() {
        println!(
            "Device: path:({}) ID_FS_TYPE:(\x1b[38;5;2m{}\x1b[0m)",
            // gives tye sysfs path (under /sys dir)
            device.syspath().display(),
            // reads properties like ID_FS_TYPE or ID_MODEL etc...
            device
                .property_value("ID_FS_TYPE")
                .unwrap_or_else(|| { OsStr::new("\x1b[38;5;3mNONE\x1b[0m") })
                .display()
        );
    }
    // INFO: That pattern works for any subsystem
}

// FIX: Check why it is printing double for each usb device
fn monitor_usb() {
    // creates new Monitor
    let monitor = MonitorBuilder::new().unwrap();
    // filters events only to usb ones
    let monitor = monitor.match_subsystem("usb").unwrap();
    // returns an object that implements iterator which can be iterated for events
    let socket = monitor.listen().unwrap();

    let mut event_iter = socket.iter();
    loop {
        if let Some(event) = event_iter.next() {
            // gets the EventType enum
            match event.event_type() {
                udev::EventType::Add => println!("USB Device - Add"),
                udev::EventType::Change => println!("USB Device - Change"),
                udev::EventType::Remove => println!("USB Device - Remove"),
                udev::EventType::Bind => println!("USB Device - Bind"),
                udev::EventType::Unbind => println!("USB Device - Unbind"),
                udev::EventType::Unknown => println!("USB Device - Unknown"),
            }
        }
    }
}

#[allow(unused)]
fn device_properties(device: udev::Device) {
    // You can access property using method
    println!("Sysname: {:?}", device.sysname());
    println!("Devnode: {:?}", device.devnode());
    println!("Subsystem: {:?}", device.subsystem());
    println!("Driver: {:?}", device.driver());

    // Or iterate over properties
    for property in device.properties() {
        println!("{:?} = {:?}", property.name(), property.value());
    }
}

fn udev_with_drm_kms() {
    let mut enumerator = udev::Enumerator::new().unwrap();
    enumerator.match_subsystem("drm").unwrap();

    for device in enumerator.scan_devices().unwrap() {
        if let Some(devnode) = device.devnode() {
            println!("DRM Device: {}", devnode.display());
            println!("Driver: {:?}", device.driver());
            println!(
                "Card type: {:?}",
                device.property_value("ID_DRM_DEVICE_TYPE")
            );
            println!();
        }
    }
}

// NOTE: Based on that experiment socket accumulates its events and you can iterate over them any
// time you want (Removing that event from socket when iterating over it).
const SLEEP_TIME: f64 = 5.0;
fn monitor_usb_wait() {
    let monitor = udev::MonitorBuilder::new()
        .unwrap()
        .match_subsystem("usb")
        .unwrap();
    let socket = monitor.listen().unwrap();

    println!("Sleep ({SLEEP_TIME}s)");
    std::thread::sleep(std::time::Duration::from_secs_f64(SLEEP_TIME));

    println!("[1] Start-ed listening");
    for event in socket.iter() {
        println!("Event: {:?}", event.event_type());
    }
    println!("[1] Done");

    println!("Sleep ({SLEEP_TIME}s)");
    std::thread::sleep(std::time::Duration::from_secs_f64(SLEEP_TIME));

    println!("[2] Start-ed listening");
    for event in socket.iter() {
        println!("Event: {:?}", event.event_type());
    }
    println!("[2] Done");
}

fn query_drm_output_devices() {
    let mut enumerator = udev::Enumerator::new().unwrap();
    enumerator.match_subsystem("drm").unwrap();
    let list = enumerator.scan_devices().unwrap();
    for device in list {
        if device.property_value("ID_FOR_SEAT").is_some()
            // Double check since enumerator already filters subsystems
            && device.subsystem() == Some(OsStr::new("drm")) 
            // Checks if device is monitor
            && device.devtype() == Some(OsStr::new("drm_minor"))
        {
            println!(
                "Output device node: {:?}, name {}",
                device.devnode(),
                device.sysname().display(),
            );
        }
    }
}

fn listen_for_hotplugged_monitors() {
    let monitor = udev::MonitorBuilder::new()
        .unwrap()
        .match_subsystem("drm")
        .unwrap();
    let socket = monitor.listen().unwrap();
    let mut event_iter = socket.iter();
    loop {
        if let Some(event) = event_iter.next() {
            let event_type = event.event_type();
            if event_type == EventType::Add || event_type == EventType::Change {
                for property in event.properties() {
                    println!("{:?} - {:?}", property.name(), property.value());
                }
                println!();
            }
        }
    }
}

fn listen_for_hotplugged_monitors_plug_unplug() {
    let monitor = MonitorBuilder::new()
        .unwrap()
        .match_subsystem("drm")
        .unwrap();
    let socket = monitor.listen().unwrap();
    let mut event_iter = socket.iter();

    loop {
        if let Some(event) = event_iter.next() {
            let event_type = event.event_type();
            if event_type == EventType::Change || event_type == EventType::Add {
                if let Some(devpath) = event.syspath().to_str() {
                    if let Some(name) = devpath.split("drm/").last() {
                        let status_path = format!("/sys/class/drm/{}/status", name);
                        println!("[1]status_path = {status_path}");
                        let device = event.device();
                        let status_path = PathBuf::from(device.syspath());
                        println!("[2]status_path = {status_path:?}");

                        let hotplug = event.property_value("HOTPLUG");
                        println!("hotplug: {hotplug:?}");

                        if let Ok(status) = std::fs::read_to_string(&status_path) {
                            match status.trim() {
                                "connected" => println!("{} plugged in", name),
                                "disconnected" => println!("{} unplugged", name),
                                "unknown" => println!("{} state unknown", name),
                                _ => println!("match error"),
                            }
                        } else {
                            println!("read error");
                        }
                    } else {
                        println!("name error");
                    }
                } else {
                    println!("Devpath error");
                }
            }
        }
    }
}
