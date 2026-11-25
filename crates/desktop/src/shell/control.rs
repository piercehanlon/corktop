use super::state_directory;
use interprocess::local_socket::{Listener, ListenerOptions, Stream, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Debug, Serialize, Deserialize)]
pub enum Launch {
    Open {
        path: Option<PathBuf>,
        desktop: Option<bool>,
    },
}

#[derive(Debug)]
pub enum Action {
    Launch(Launch),
    ToggleVisible,
    ToggleLayer,
    ChooseFolder,
    Panel,
    Settings,
    Quit,
    Chosen(PathBuf),
    Error(String),
    PickerClosed,
}

#[derive(Clone)]
pub struct Sender {
    tx: mpsc::Sender<Action>,
    context: Arc<Mutex<Option<eframe::egui::Context>>>,
}
impl Sender {
    pub fn send(&self, a: Action) {
        if self.tx.send(a).is_ok()
            && let Ok(ctx) = self.context.lock()
            && let Some(ctx) = ctx.as_ref()
        {
            ctx.request_repaint();
        }
    }
}

pub struct Control {
    pub sender: Sender,
    rx: mpsc::Receiver<Action>,
}
impl Control {
    pub fn local() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            sender: Sender {
                tx,
                context: Default::default(),
            },
            rx,
        }
    }
    pub fn attach(&self, ctx: &eframe::egui::Context) {
        *self.sender.context.lock().unwrap() = Some(ctx.clone());
    }
    pub fn events(&self) -> impl Iterator<Item = Action> + '_ {
        self.rx.try_iter()
    }
}

fn read_message(stream: &mut Stream) -> io::Result<Vec<u8>> {
    stream.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut message = Vec::new();
    let mut chunk = [0; 1024];
    while Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                message.extend_from_slice(&chunk[..n]);
                if message.len() > 65536 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                if message.ends_with(b"\n") {
                    stream.set_nonblocking(false)?;
                    return Ok(message);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || (cfg!(windows) && e.raw_os_error() == Some(232)) =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e),
        }
    }
    Err(io::ErrorKind::TimedOut.into())
}

fn listen(listener: Listener, lock: fs::File, sender: Sender) -> io::Result<()> {
    std::thread::Builder::new()
        .name("launch-router".into())
        .spawn(move || {
            let _lock = lock;
            for mut stream in listener.incoming().flatten() {
                if let Ok(line) = read_message(&mut stream)
                    && let Ok(request) = serde_json::from_slice(&line)
                {
                    sender.send(Action::Launch(request));
                    let _ = stream.write_all(b"OK\n");
                }
            }
        })?;
    Ok(())
}

pub fn start(request: Launch) -> io::Result<Option<Control>> {
    let dir = state_directory()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&dir)?;
    }
    #[cfg(windows)]
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    let endpoint = dir
        .join("launch.sock")
        .to_fs_name::<interprocess::local_socket::GenericFilePath>()?;
    #[cfg(windows)]
    let endpoint = {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        dir.hash(&mut hash);
        format!("corktop-{:016x}", hash.finish())
            .to_ns_name::<interprocess::local_socket::GenericNamespaced>()?
    };
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(false)
        .create(true)
        .open(dir.join("primary.lock"))?;
    match lock.try_lock() {
        Ok(()) => {
            let listener = ListenerOptions::new()
                .name(endpoint)
                .try_overwrite(true)
                .create_sync()?;
            let control = Control::local();
            listen(listener, lock, control.sender.clone())?;
            Ok(Some(control))
        }
        Err(fs::TryLockError::WouldBlock) => {
            let mut message = serde_json::to_vec(&request)?;
            message.push(b'\n');
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut stream = loop {
                match Stream::connect(endpoint.borrow()) {
                    Ok(s) => break s,
                    Err(_) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(e) => return Err(e),
                }
            };
            stream.write_all(&message)?;
            if read_message(&mut stream)? != b"OK\n" {
                return Err(io::Error::other(
                    "existing window did not acknowledge launch",
                ));
            }
            Ok(None)
        }
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}
