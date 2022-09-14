use std::path::Path;

use nix::{
    errno::Errno,
    mount::{mount as nix_mount, MsFlags},
};

pub fn bind<P1: AsRef<Path>, P2: AsRef<Path>>(source: P1, target: P2) -> Result<(), Errno> {
    mount(
        Some(source.as_ref()),
        target,
        Some("none"),
        Some(MsFlags::MS_BIND | MsFlags::MS_PRIVATE | MsFlags::MS_REC),
        None as Option<&str>,
    )
}

pub fn mount<
    P1: AsRef<Path> + ?Sized,
    P2: AsRef<Path>,
    T: AsRef<str> + ?Sized,
    D: AsRef<str> + ?Sized,
>(
    source: Option<&P1>,
    target: P2,
    fstype: Option<&T>,
    flags: Option<MsFlags>,
    data: Option<&D>,
) -> Result<(), Errno> {
    let source = match source {
        Some(s) => Some(s.as_ref()),
        None => None,
    };

    let fstype = match fstype {
        Some(t) => Some(t.as_ref()),
        None => None,
    };

    let data = match data {
        Some(d) => Some(d.as_ref()),
        None => None,
    };

    let flags = match flags {
        Some(f) => f,
        None => MsFlags::empty(),
    };
    nix_mount(source, target.as_ref(), fstype, flags, data)?;

    Ok({})
}
