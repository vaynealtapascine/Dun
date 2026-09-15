//! COM class that Windows calls when a toast (or one of its buttons) is
//! clicked, whether the toast is on screen or sitting in Action Center.
//!
//! The class object is registered on a dedicated multithreaded-apartment
//! thread that parks for the life of the process. MTA calls arrive on COM's
//! own RPC threads, so nothing here needs a message loop.

use std::ffi::c_void;
use std::sync::{mpsc, Arc};

use windows::core::{implement, IUnknown, Interface, Ref, BOOL, GUID, PCWSTR};
use windows::Win32::Foundation::CLASS_E_NOAGGREGATION;
use windows::Win32::System::Com::{
    CoInitializeEx, CoRegisterClassObject, IClassFactory, IClassFactory_Impl, CLSCTX_LOCAL_SERVER,
    COINIT_MULTITHREADED, REGCLS_MULTIPLEUSE,
};
use windows::Win32::UI::Notifications::{
    INotificationActivationCallback, INotificationActivationCallback_Impl,
    NOTIFICATION_USER_INPUT_DATA,
};

/// Receives the raw `arguments` string of whatever was clicked.
pub type Sink = Arc<dyn Fn(String) + Send + Sync>;

#[implement(INotificationActivationCallback)]
struct Activator {
    sink: Sink,
}

impl INotificationActivationCallback_Impl for Activator_Impl {
    fn Activate(
        &self,
        _appusermodelid: &PCWSTR,
        invokedargs: &PCWSTR,
        _data: *const NOTIFICATION_USER_INPUT_DATA,
        _count: u32,
    ) -> windows::core::Result<()> {
        let args = if invokedargs.is_null() {
            String::new()
        } else {
            unsafe { invokedargs.to_string() }.unwrap_or_default()
        };
        (self.sink)(args);
        Ok(())
    }
}

#[implement(IClassFactory)]
struct Factory {
    sink: Sink,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<'_, IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        unsafe { *ppvobject = std::ptr::null_mut() };
        if !punkouter.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let activator: INotificationActivationCallback = Activator {
            sink: self.sink.clone(),
        }
        .into();
        unsafe { activator.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Registers the class object and returns once it is live, so a launch with
/// `-ToastActivated` can't miss the activation that caused it.
pub fn start(clsid: GUID, sink: Sink) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("toast-activator".into())
        .spawn(move || {
            let registered = unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .map_err(|e| format!("CoInitializeEx: {e}"))
                    .and_then(|_| {
                        let factory: IClassFactory = Factory { sink }.into();
                        CoRegisterClassObject(
                            &clsid,
                            &factory,
                            CLSCTX_LOCAL_SERVER,
                            REGCLS_MULTIPLEUSE,
                        )
                        .map(|cookie| (factory, cookie))
                        .map_err(|e| format!("CoRegisterClassObject: {e}"))
                    })
            };
            match registered {
                Ok((_factory, _cookie)) => {
                    let _ = ready_tx.send(Ok(()));
                    // Keep the apartment and the registration alive for the process lifetime.
                    loop {
                        std::thread::park();
                    }
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        })
        .map_err(|e| format!("spawn toast-activator: {e}"))?;

    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("toast activator did not start: {e}"))?
}
