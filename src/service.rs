#[cfg(windows)]
pub mod windows {
    use std::ffi::OsString;
    use std::sync::Arc;

    use std::time::Duration;
    use windows_service::service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use interprocess::local_socket::traits::ListenerExt;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    use crate::scanner::{scan_drive_mft, ScanState};
    use crate::service_ipc::windows::{
        create_listener, recv_request, send_response, FileEntryWire, Request, Response,
    };

    pub const SERVICE_NAME: &str = "StorageCleanerScan";
    const SERVICE_DISPLAY_NAME: &str = "Storage Cleaner MFT Scanner";

    fn try_register_service_handler() -> windows_service::Result<()> {
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop | ServiceControl::Interrogate => {
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };
        let status_handle =
            service_control_handler::register(SERVICE_NAME, event_handler)?;
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;
        Ok(())
    }

    pub fn install_service() -> windows_service::Result<()> {
        let exe = std::env::current_exe().map_err(windows_service::Error::Winapi)?;
        let manager =
            ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)?;
        let info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::OnDemand,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe,
            launch_arguments: vec![OsString::from("--service")],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };
        manager.create_service(&info, ServiceAccess::CHANGE_CONFIG)?;
        Ok(())
    }

    pub fn uninstall_service() -> windows_service::Result<()> {
        let manager =
            ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::DELETE)?;
        let _ = service.stop();
        service.delete()?;
        Ok(())
    }

    pub fn run_pipe_server() {
        let _ = try_register_service_handler();

        let listener = match create_listener() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to create pipe listener: {}", e);
                return;
            }
        };

        for stream_result in listener.incoming() {
            let mut stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    continue;
                }
            };

            std::thread::spawn(move || {
                let req = match recv_request(&mut stream) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = send_response(&mut stream, &Response::Error(e.to_string()));
                        return;
                    }
                };

                match req {
                    Request::Scan { drive } => {
                        let state = Arc::new(std::sync::Mutex::new(ScanState::default()));
                        match scan_drive_mft(drive, &state) {
                            Some(entries) => {
                                for e in &entries {
                                    let wire = FileEntryWire::from(e);
                                    if send_response(&mut stream, &Response::File(wire)).is_err() {
                                        break;
                                    }
                                }
                                let _ = send_response(&mut stream, &Response::Done);
                            }
                            None => {
                                let _ = send_response(
                                    &mut stream,
                                    &Response::Error("MFT scan failed".to_string()),
                                );
                            }
                        }
                    }
                }
            });
        }
    }
}
