pub mod re_animatr {

    use std::cmp::{Eq, PartialEq};
    use std::collections::HashMap;
    use std::fmt::Display;
    use std::hash::Hash;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, SystemTime};

    pub struct TTLBuffer<Key>
    where
        Key: Eq + Hash,
    {
        pub timeout: Duration,
        pub interval: Duration,
        map: Arc<Mutex<HashMap<Key, SystemTime>>>,
        sender: Sender<Key>,
        pub receiver: Receiver<Key>,
    }

    pub struct TTLItem<T> {
        pub payload: T,
    }

    impl<Key> TTLBuffer<Key>
    where
        Key: Eq + Hash + Sync + Send + Display + PartialEq + Clone + 'static,
    {
        pub fn add(&mut self, key: Key) {
            self.map
                .lock()
                .unwrap()
                .entry(key)
                .and_modify(|x| *x = SystemTime::now())
                .or_insert(SystemTime::now());
        }

        pub fn len(&self) -> usize {
            self.map.lock().unwrap().len()
        }

        fn start(&self)
        where
            Key: Eq + Hash,
        {
            let thread_sender = self.sender.clone();
            let interval = self.interval;
            let thread_map_mutex = self.map.clone();
            let g_timeout = self.timeout;
            thread::spawn(move || loop {
                let mut keys_to_remove = vec![];

                match thread_map_mutex.lock() {
                    Ok(t_map) => {
                        let now = SystemTime::now();

                        for (k, v) in t_map.iter() {
                            match now.duration_since(v.clone()) {
                                Ok(timeout) if timeout > g_timeout => {
                                    match thread_sender.send(k.clone()) {
                                        Ok(_) => {}
                                        Err(err) => {
                                            println!("Error when sending to channel: {}", err)
                                        }
                                    };
                                    keys_to_remove.push(k.clone());
                                }
                                Ok(_) => {}
                                Err(err) => panic!("Error: {}", err),
                            };
                        }
                    }
                    Err(err) => println!("Could not acquire map lock: {}", err),
                };

                let mut t_map = thread_map_mutex.lock().unwrap();
                for k in keys_to_remove {
                    t_map.remove(&k);
                }
                thread::sleep(interval);
            });
        }
    }

    pub fn new<Key>(timeout: Duration, interval: Duration) -> TTLBuffer<Key>
    where
        Key: Eq + Hash + Sync + Send + Display + Clone + 'static,
    {
        let (sender, receiver) = channel();
        let buffer = TTLBuffer {
            timeout: timeout,
            interval: interval,
            map: Arc::new(Mutex::new(HashMap::new())),
            sender: sender,
            receiver: receiver,
        };

        buffer.start();

        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use re_animatr::TTLBuffer;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_new() {
        let buffer: TTLBuffer<&str> =
            re_animatr::new(Duration::from_millis(32), Duration::from_millis(500));
        assert_eq!(buffer.timeout, Duration::from_millis(32));
    }

    #[test]
    fn test_add() {
        let mut buf: TTLBuffer<&str> =
            re_animatr::new(Duration::from_millis(32), Duration::from_millis(500));
        buf.add("xxx");
    }

    #[test]
    fn test_receive() {
        let mut buf: TTLBuffer<&str> =
            re_animatr::new(Duration::from_millis(320), Duration::from_millis(50));
        buf.add("xxx");
        let t = buf.receiver.recv().unwrap();
        assert_eq!("xxx", t);

        buf.add("yyy");
        let t = buf.receiver.recv().unwrap();
        assert_eq!("yyy", t);
    }

    #[test]
    fn test_receive2() {
        let mut buf: TTLBuffer<&str> =
            re_animatr::new(Duration::from_millis(320), Duration::from_millis(50));
        buf.add("xxx");
        buf.add("yyy");

        let mut responses = vec![];

        responses.push(buf.receiver.recv().unwrap());
        responses.push(buf.receiver.recv().unwrap());

        assert_eq!(responses.len(), 2);

        let filtered = responses.iter().filter(|&r| r == &"xxx" || r == &"yyy");
        assert_eq!(filtered.size_hint(), (0, Some(2)));
    }

    #[test]
    fn test_update() {
        let mut buf: TTLBuffer<&str> =
            re_animatr::new(Duration::from_millis(100), Duration::from_millis(10));

        let mut responses = vec![];

        buf.add("xxx"); // Add an entry with key "xxx"
        buf.add("yyy"); // Add an entry with key "yyy"
        thread::sleep(Duration::from_millis(80)); // Wait for some part of the timeout
        assert_eq!(buf.len(), 2); // Make sure the values are still in the buffer

        buf.add("xxx"); // Update the "xxx" entry we created above, note that we don't update the "yyy" entry
        thread::sleep(Duration::from_millis(80)); // Sleep for the duration of "yyy", but not "xxx", since that was updated
        assert_eq!(buf.len(), 1); // There should now be only one value in the buffer

        responses.push(buf.receiver.recv().unwrap()); // Receive the "yyy" entry and store it in responses vector
        assert_eq!("yyy", responses[0]); // Make sure that it was really "yyy" that we received

        thread::sleep(Duration::from_millis(80)); // Sleep for the reminder of the "xxx" timeout
        responses.push(buf.receiver.recv().unwrap()); // Receive the "xxx" entry and store it in the buffer
        assert_eq!(buf.len(), 0); // Make sure that the buffer is empty (both entries have been sent)
        assert_eq!("xxx", responses[1]); // Make sure that it was really "xxx" that we received
    }
}
