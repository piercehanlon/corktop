use std::io;
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt, MapState, PropMode},
    wrapper::ConnectionExt as _,
};

pub(super) fn set(window: u32, desktop: bool) -> io::Result<()> {
    let (conn, screen) = x11rb::connect(None).map_err(io::Error::other)?;
    let atom = |name: &[u8]| -> io::Result<u32> {
        Ok(conn
            .intern_atom(false, name)
            .map_err(io::Error::other)?
            .reply()
            .map_err(io::Error::other)?
            .atom)
    };
    let property = atom(b"_NET_WM_WINDOW_TYPE")?;
    let kind = if desktop {
        atom(b"_NET_WM_WINDOW_TYPE_DESKTOP")?
    } else {
        atom(b"_NET_WM_WINDOW_TYPE_NORMAL")?
    };
    if desktop {
        let root = conn.setup().roots[screen].root;
        let supported = conn
            .get_property(
                false,
                root,
                atom(b"_NET_SUPPORTED")?,
                AtomEnum::ATOM,
                0,
                4096,
            )
            .map_err(io::Error::other)?
            .reply()
            .map_err(io::Error::other)?;
        if !supported
            .value32()
            .is_some_and(|mut a| a.any(|v| v == kind))
        {
            return Err(io::Error::other(
                "This window manager does not advertise desktop windows. Standard window is active.",
            ));
        }
    }
    // Window managers inspect this hint when managing a newly mapped client.
    // Withdraw and remap so they also update stacking and permitted actions.
    let mapped = conn
        .get_window_attributes(window)
        .map_err(io::Error::other)?
        .reply()
        .map_err(io::Error::other)?
        .map_state
        != MapState::UNMAPPED;
    if mapped {
        conn.unmap_window(window)
            .map_err(io::Error::other)?
            .check()
            .map_err(io::Error::other)?;
    }
    let change = conn
        .change_property32(PropMode::REPLACE, window, property, AtomEnum::ATOM, &[kind])
        .map_err(io::Error::other)?
        .check()
        .map_err(io::Error::other);
    if mapped {
        conn.map_window(window)
            .map_err(io::Error::other)?
            .check()
            .map_err(io::Error::other)?;
    }
    change?;
    conn.flush().map_err(io::Error::other)
}
