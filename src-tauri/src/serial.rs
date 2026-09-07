use std::collections::BTreeSet;

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
