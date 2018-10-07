pub mod re_animatr {

    use std::cmp::{Eq, PartialEq};
    use std::collections::HashMap;
    use std::fmt::Display;
    use std::hash::Hash;
    use std::sync::mpsc::{Receiver, Sender, channel};
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
        pub fn add(&mut self, key: Key) -> Option<SystemTime> {
            self.map.lock().unwrap().insert(key, SystemTime::now())
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

    use std::time::Duration;
    use re_animatr::TTLBuffer;

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
        assert_eq!(buf.add("xxx"), None);
    }

    #[test]
    fn test_receive() {
        let mut buf: TTLBuffer<&str> =
            re_animatr::new(Duration::from_millis(320), Duration::from_millis(50));
        assert_eq!(buf.add("xxx"), None);
        let t = buf.receiver.recv().unwrap();
        assert_eq!("xxx", t);

        assert_eq!(buf.add("yyy"), None);
        let t = buf.receiver.recv().unwrap();
        assert_eq!("yyy", t);
    }

    #[test]
    fn test_receive2() {
        let mut buf: TTLBuffer<&str> =
            re_animatr::new(Duration::from_millis(320), Duration::from_millis(50));
        assert_eq!(buf.add("xxx"), None);
        assert_eq!(buf.add("yyy"), None);

        let mut responses = vec![];

        responses.push(buf.receiver.recv().unwrap());
        responses.push(buf.receiver.recv().unwrap());

        assert_eq!(responses.len(), 2);

        let filtered = responses.iter().filter(|&r| r == &"xxx" || r == &"yyy");
        assert_eq!(filtered.size_hint(), (0, Some(2)));
    }
}
