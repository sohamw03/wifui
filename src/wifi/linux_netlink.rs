use crate::error::{WifiError, WifiResult};
use crate::wifi::types::EthernetStatus;
use futures_lite::StreamExt;
use rtnetlink::{
    Handle, IpVersion, RouteMessageBuilder, new_connection,
    packet_route::link::{LinkAttribute, LinkFlags, LinkLayerType, LinkMessage, State},
    packet_route::route::{RouteAttribute, RouteHeader, RouteMessage},
};
use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Mutex, OnceLock};

static NETLINK_RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
static NETLINK_HANDLE: OnceLock<Result<Handle, String>> = OnceLock::new();
static NETLINK_QUERY_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn get_status() -> WifiResult<EthernetStatus> {
    let handle = netlink_handle()?;
    let runtime = netlink_runtime()?;
    let _guard = NETLINK_QUERY_LOCK
        .lock()
        .map_err(|error| query_error("lock the netlink query", error))?;
    runtime
        .block_on(get_status_async(handle))
        .map_err(|error| query_error("query Linux network state", error))
}

fn netlink_runtime() -> WifiResult<&'static tokio::runtime::Runtime> {
    let result = NETLINK_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
    });
    result
        .as_ref()
        .map_err(|error| query_error("create the netlink runtime", error))
}

fn netlink_handle() -> WifiResult<&'static Handle> {
    let runtime = netlink_runtime()?;
    let result = NETLINK_HANDLE.get_or_init(|| {
        runtime.block_on(async {
            let (connection, handle, mut messages) =
                new_connection().map_err(|error| error.to_string())?;
            runtime.spawn(connection);
            runtime.spawn(async move { while messages.next().await.is_some() {} });
            Ok::<Handle, String>(handle)
        })
    });
    result
        .as_ref()
        .map_err(|error| query_error("initialize the netlink connection", error))
}

async fn get_status_async(handle: &Handle) -> Result<EthernetStatus, String> {
    let active_interfaces = active_ethernet_indices(handle).await?;
    if active_interfaces.is_empty() {
        return Ok(EthernetStatus::Inactive);
    }

    let ethernet_owns_ipv4 = default_route_uses_ethernet(handle, IpVersion::V4, &active_interfaces)
        .await
        .unwrap_or(false);
    let ethernet_owns_ipv6 = default_route_uses_ethernet(handle, IpVersion::V6, &active_interfaces)
        .await
        .unwrap_or(false);

    if ethernet_owns_ipv4 || ethernet_owns_ipv6 {
        Ok(EthernetStatus::Connected)
    } else {
        Ok(EthernetStatus::Active)
    }
}

async fn active_ethernet_indices(handle: &Handle) -> Result<HashSet<u32>, String> {
    let mut links = handle.link().get().execute();
    let mut active_interfaces = HashSet::new();
    while let Some(link) = links.try_next().await.map_err(|error| error.to_string())? {
        if is_active_ethernet(&link) {
            active_interfaces.insert(link.header.index);
        }
    }
    Ok(active_interfaces)
}

fn is_active_ethernet(link: &LinkMessage) -> bool {
    let carrier_is_up = link.header.flags.contains(LinkFlags::LowerUp)
        || link.attributes.iter().any(|attribute| {
            matches!(
                attribute,
                LinkAttribute::Carrier(1) | LinkAttribute::OperState(State::Up)
            )
        });
    link.header.link_layer_type == LinkLayerType::Ether
        && link.header.flags.contains(LinkFlags::Up)
        && carrier_is_up
        && !link.header.flags.contains(LinkFlags::Loopback)
}

async fn default_route_uses_ethernet(
    handle: &Handle,
    ip_version: IpVersion,
    ethernet_interfaces: &HashSet<u32>,
) -> Result<bool, rtnetlink::Error> {
    let route = match ip_version {
        IpVersion::V4 => RouteMessageBuilder::<Ipv4Addr>::new().build(),
        IpVersion::V6 => RouteMessageBuilder::<Ipv6Addr>::new().build(),
    };
    let mut routes = handle.route().get(route).execute();
    let mut best_metric = None;
    let mut best_is_ethernet = false;

    while let Some(route) = routes.try_next().await? {
        if !is_default_route(&route) {
            continue;
        }

        let Some(interface) = route_interface(&route) else {
            continue;
        };
        let metric = route_metric(&route);
        let is_ethernet = ethernet_interfaces.contains(&interface);

        if best_metric.map_or(true, |best| metric < best) {
            best_metric = Some(metric);
            best_is_ethernet = is_ethernet;
        } else if best_metric == Some(metric) && is_ethernet {
            best_is_ethernet = true;
        }
    }

    Ok(best_is_ethernet)
}

fn is_default_route(route: &RouteMessage) -> bool {
    if route.header.destination_prefix_length != 0 {
        return false;
    }
    let table = route
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            RouteAttribute::Table(table) => Some(*table),
            _ => None,
        })
        .unwrap_or(u32::from(route.header.table));
    table == u32::from(RouteHeader::RT_TABLE_MAIN)
}

fn route_interface(route: &RouteMessage) -> Option<u32> {
    route
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            RouteAttribute::Oif(interface) => Some(*interface),
            _ => None,
        })
}

fn route_metric(route: &RouteMessage) -> u32 {
    route
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            RouteAttribute::Priority(metric) => Some(*metric),
            _ => None,
        })
        .unwrap_or_default()
}

fn query_error(operation: &str, reason: impl ToString) -> WifiError {
    WifiError::EthernetStatusQueryFailed {
        operation: operation.to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_ethernet_requires_admin_and_carrier_state() {
        let mut link = LinkMessage::default();
        assert!(!is_active_ethernet(&link));

        link.header.link_layer_type = LinkLayerType::Ether;
        link.header.flags = LinkFlags::Up;
        assert!(!is_active_ethernet(&link));

        link.header.flags |= LinkFlags::LowerUp;
        assert!(is_active_ethernet(&link));

        link.header.flags &= !LinkFlags::LowerUp;
        link.attributes.push(LinkAttribute::OperState(State::Up));
        assert!(is_active_ethernet(&link));

        link.header.flags |= LinkFlags::Loopback;
        assert!(!is_active_ethernet(&link));
    }

    #[test]
    fn identifies_main_default_route_and_its_interface_metric() {
        let mut route = RouteMessage::default();
        route.header.destination_prefix_length = 0;
        route.header.table = RouteHeader::RT_TABLE_MAIN;
        route.attributes.push(RouteAttribute::Oif(7));
        route.attributes.push(RouteAttribute::Priority(42));

        assert!(is_default_route(&route));
        assert_eq!(route_interface(&route), Some(7));
        assert_eq!(route_metric(&route), 42);

        route.header.destination_prefix_length = 24;
        assert!(!is_default_route(&route));
    }
}
