//! Which firewall profile Windows has put this PC's networks on.
//!
//! Dun's rule allows the sync port on private and domain networks only —
//! opening a port to a coffee-shop network isn't worth it. But Windows marks
//! plenty of home networks Public, and then the server can be listening, the
//! phone paired, and nothing ever gets through. Nobody would guess why, so the
//! UI says it.

/// Names of the connected networks Windows treats as Public.
#[cfg(windows)]
pub fn public_networks() -> Vec<String> {
    // On its own thread: this initialises COM, and the caller's apartment is
    // Tauri's to decide.
    std::thread::spawn(|| unsafe { enumerate() }.unwrap_or_default())
        .join()
        .unwrap_or_default()
}

#[cfg(not(windows))]
pub fn public_networks() -> Vec<String> {
    Vec::new()
}

#[cfg(windows)]
unsafe fn enumerate() -> windows::core::Result<Vec<String>> {
    use windows::Win32::Networking::NetworkListManager::{
        INetwork, INetworkListManager, NetworkListManager, NLM_ENUM_NETWORK_CONNECTED,
        NLM_NETWORK_CATEGORY_PUBLIC,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let result = (|| {
        let manager: INetworkListManager =
            unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL) }?;
        let networks = unsafe { manager.GetNetworks(NLM_ENUM_NETWORK_CONNECTED) }?;

        let mut public = Vec::new();
        loop {
            let mut fetched = [const { None::<INetwork> }; 1];
            let mut count = 0u32;
            unsafe { networks.Next(&mut fetched, Some(&mut count)) }?;
            if count == 0 {
                break;
            }
            let Some(network) = fetched[0].take() else {
                break;
            };
            if unsafe { network.GetCategory() }? == NLM_NETWORK_CATEGORY_PUBLIC {
                let name = unsafe { network.GetName() }
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| "this network".to_string());
                public.push(name);
            }
        }
        Ok(public)
    })();
    if init.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn asking_is_safe_wherever_it_runs() {
        // Whatever this machine's networks are, the answer must be names and
        // must not panic or hang — it's on the path of every sync status call.
        for name in super::public_networks() {
            assert!(!name.is_empty());
        }
    }
}
