use std::collections::BTreeSet;

#[cfg(target_os = "windows")]
pub(crate) fn set_windows_write_timeout(
    port: &serialport::COMPort,
    timeout_ms: u32,
) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Devices::Communication::{
        COMMTIMEOUTS, GetCommTimeouts, SetCommTimeouts,
    };

    let handle = port.as_raw_handle();
    let mut timeouts = COMMTIMEOUTS {
        ReadIntervalTimeout: 0,
        ReadTotalTimeoutMultiplier: 0,
        ReadTotalTimeoutConstant: 0,
        WriteTotalTimeoutMultiplier: 0,
        WriteTotalTimeoutConstant: 0,
    };
    // serialport sets both read and write deadlines from one timeout. Preserve
    // its short read polling interval while allowing USB writes to drain.
    if unsafe { GetCommTimeouts(handle, &mut timeouts) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    timeouts.WriteTotalTimeoutMultiplier = 0;
    timeouts.WriteTotalTimeoutConstant = timeout_ms;
    if unsafe { SetCommTimeouts(handle, &timeouts) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn collapse_serial_port_aliases(
    ports: Vec<serialport::SerialPortInfo>,
) -> Vec<serialport::SerialPortInfo> {
    let callout_suffixes = ports
        .iter()
        .filter_map(|port| port.port_name.strip_prefix("/dev/cu."))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    ports
        .into_iter()
        .filter(|port| match port.port_name.strip_prefix("/dev/tty.") {
            Some(suffix) => !callout_suffixes.contains(suffix),
            None => true,
        })
        .collect()
}
