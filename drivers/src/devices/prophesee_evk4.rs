use crate::adapters;
use crate::configuration;
use crate::device;
use crate::error;
use crate::properties;
use crate::usb;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Biases {
    pub pr: u8,
    pub fo: u8,
    pub hpf: u8,
    pub diff_on: u8,
    pub diff: u8,
    pub diff_off: u8,
    pub inv: u8,
    pub refr: u8,
    pub reqpuy: u8,
    pub reqpux: u8,
    pub sendreqpdy: u8,
    pub unknown_1: u8,
    pub unknown_2: u8,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Configuration {
    pub biases: Biases,
    pub x_mask: [u64; 20],
    pub y_mask: [u64; 12],
    pub invert_mask: bool,
    pub enable_external_trigger: bool,
}

pub struct Device {
    handle: std::sync::Arc<rusb::DeviceHandle<rusb::Context>>,
    ring: usb::Ring,
    configuration_updater: configuration::Updater<Configuration>,
    serial: String,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum Error {
    #[error("{0}")]
    Usb(#[from] usb::Error),

    #[error("short write ({requested} bytes requested, {written} bytes written)")]
    ShortWrite { requested: usize, written: usize },

    #[error("short response while reading register {0}")]
    RegisterReadShortResponse(u32),

    #[error("bytes mismatch while reading register {0}")]
    RegisterReadMismatch(u32),
}

impl From<rusb::Error> for Error {
    fn from(error: rusb::Error) -> Self {
        usb::Error::from(error).into()
    }
}

impl device::Usb for Device {
    type Adapter = adapters::evt3::Adapter;

    type Configuration = Configuration;

    type Error = Error;

    type Properties = properties::Camera<Self::Configuration>;

    const VENDOR_ID: u16 = 0x04b4;

    const PRODUCT_ID: u16 = 0x00f5;

    const PROPERTIES: Self::Properties = Self::Properties {
        name: "Prophesee EVK4",
        width: 1280,
        height: 720,
        default_configuration: Self::Configuration {
            biases: Biases {
                pr: 0x7C,
                fo: 0x53,
                hpf: 0x00,
                diff_on: 0x66,
                diff: 0x4D,
                diff_off: 0x49,
                inv: 0x5B,
                refr: 0x14,
                reqpuy: 0x8C,
                reqpux: 0x7C,
                sendreqpdy: 0x94,
                unknown_1: 0x74,
                unknown_2: 0x51,
            },
            x_mask: [0; 20],
            y_mask: [0; 12],
            invert_mask: false,
            enable_external_trigger: true,
        },
    };

    const DEFAULT_USB_CONFIGURATION: usb::Configuration = usb::Configuration {
        buffer_size: 1 << 17,
        ring_size: 1 << 12,
        transfer_queue_size: 1 << 5,
        allow_dma: false,
    };

    fn read_serial(handle: &mut rusb::DeviceHandle<rusb::Context>) -> rusb::Result<String> {
        handle.claim_interface(0)?;
        handle.write_bulk(
            0x02,
            &[0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            std::time::Duration::from_secs(1),
        )?;
        let mut buffer = vec![0u8; 16];
        handle.read_bulk(0x82, &mut buffer, std::time::Duration::from_secs(1))?;
        Ok(format!(
            "{:02X}{:02X}{:02X}{:02X}",
            buffer[11], buffer[10], buffer[9], buffer[8]
        ))
    }

    fn update_configuration(&self, configuration: Self::Configuration) {
        self.configuration_updater.update(configuration);
    }

    fn open<IntoError>(
        serial: &Option<&str>,
        configuration: Self::Configuration,
        usb_configuration: &usb::Configuration,
        event_loop: std::sync::Arc<usb::EventLoop>,
        error_flag: error::Flag<IntoError>,
    ) -> Result<Self, Self::Error>
    where
        IntoError: From<Self::Error> + Clone + Send + 'static,
    {
        let (handle, serial) = Self::handle_from_serial(event_loop.context(), serial)?;
        usb::assert_control_transfer(
            &handle,
            0x80,
            0x06,
            0x0300,
            0x0000,
            &[0x04, 0x03, 0x09, 0x04],
            TIMEOUT,
        )?;
        usb::assert_control_transfer(
            &handle,
            0x80,
            0x06,
            0x0301,
            0x0409,
            &[
                0x14, 0x03, b'P', 0x00, b'r', 0x00, b'o', 0x00, b'p', 0x00, b'h', 0x00, b'e', 0x00,
                b's', 0x00, b'e', 0x00, b'e', 0x00,
            ],
            TIMEOUT,
        )?;
        usb::assert_control_transfer(
            &handle,
            0x80,
            0x06,
            0x0300,
            0x0000,
            &[0x04, 0x03, 0x09, 0x04],
            TIMEOUT,
        )?; // potentially redundant
        usb::assert_control_transfer(
            &handle,
            0x80,
            0x06,
            0x0302,
            0x0409,
            &[0x0a, 0x03, b'E', 0x00, b'V', 0x00, b'K', 0x00, b'4', 0x00],
            TIMEOUT,
        )?;
        request(
            &handle,
            &[0x79, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            TIMEOUT,
        )?; // read release version
        request(
            &handle,
            &[0x7a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            TIMEOUT,
        )?; // read build date
        request(
            &handle,
            &[0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00],
            TIMEOUT,
        )?; // ?
        request(
            &handle,
            &[
                0x03, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            TIMEOUT,
        )?; // psee,ccam5_imx636 psee,ccam5_gen42
        let _ = Reserved0014::default().read(&handle)?; // ?
        request(
            &handle,
            &[0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            TIMEOUT,
        )?; // serial request
        request(
            &handle,
            &[0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00],
            TIMEOUT,
        )?; // ?
        request(
            &handle,
            &[
                0x01, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            TIMEOUT,
        )?; // CCam5 Imx636 Event-Based Camera
        request(
            &handle,
            &[
                0x03, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            TIMEOUT,
        )?; // psee,ccam5_imx636 psee,ccam5_gen42
        let _ = Reserved0014::default().read(&handle)?; // ?

        // issd_evk3_imx636_stop in hal_psee_plugins/include/devices/imx636/imx636_evk3_issd.h {
        RoiCtrl {
            reserved_0_1: 0,
            td_enable: 1,
            reserved_2_5: 0,
            td_shadow_trigger: 0,
            td_roni_n_en: 1,
            reserved_7_10: 0,
            td_rstn: 0,
            reserved_11_32: 0x1e000a,
        }
        .write(&handle)?;
        Unknown002C { value: 0x0022c324 }.write(&handle)?;
        RoCtrl { value: 0x00000002 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_millis(1));
        let _ = TimeBaseCtrl::default().read(&handle)?;
        TimeBaseCtrl { value: 0x00000644 }.write(&handle)?;
        MipiControl { value: 0x000002f8 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(300));
        // }

        // issd_evk3_imx636_destroy in hal_psee_plugins/include/devices/imx636/imx636_evk3_issd.h {
        Unknown0070 { value: 0x00400008 }.write(&handle)?;
        Unknown006C { value: 0x0ee47114 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(500));
        UnknownA00C { value: 0x00020400 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(500));
        UnknownA010 { value: 0x00008068 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        Unknown1104 { value: 0x00000000 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownA020 { value: 0x00000050 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownA004 { value: 0x000b0500 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownA008 { value: 0x00002404 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownA000 { value: 0x000b0500 }.write(&handle)?;
        UnknownB044 { value: 0x00000000 }.write(&handle)?;
        UnknownB004 { value: 0x0000000a }.write(&handle)?;
        UnknownB040 { value: 0x0000000e }.write(&handle)?;
        UnknownB0C8 { value: 0x00000000 }.write(&handle)?;
        UnknownB040 { value: 0x00000006 }.write(&handle)?;
        UnknownB040 { value: 0x00000004 }.write(&handle)?;
        Unknown0000 { value: 0x4f006442 }.write(&handle)?;
        Unknown0000 { value: 0x0f006442 }.write(&handle)?;
        Unknown00B8 { value: 0x00000401 }.write(&handle)?;
        Unknown00B8 { value: 0x00000400 }.write(&handle)?;
        UnknownB07C { value: 0x00000000 }.write(&handle)?;
        // }

        // issd_evk3_imx636_init in hal_psee_plugins/include/devices/imx636/imx636_evk3_issd.h {
        Unknown001C { value: 0x00000001 }.write(&handle)?;
        Reset { value: 0x00000001 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        Reset { value: 0x00000000 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        MipiControl { value: 0x00000158 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        UnknownB044 { value: 0x00000000 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(300));
        UnknownB004 { value: 0x0000000a }.write(&handle)?;
        UnknownB040 { value: 0x00000000 }.write(&handle)?;
        UnknownB0C8 { value: 0x00000000 }.write(&handle)?;
        UnknownB040 { value: 0x00000000 }.write(&handle)?;
        UnknownB040 { value: 0x00000000 }.write(&handle)?;
        Unknown0000 { value: 0x4f006442 }.write(&handle)?;
        Unknown0000 { value: 0x0f006442 }.write(&handle)?;
        Unknown00B8 { value: 0x00000400 }.write(&handle)?;
        Unknown00B8 { value: 0x00000400 }.write(&handle)?;
        UnknownB07C { value: 0x00000000 }.write(&handle)?;
        UnknownB074 { value: 0x00000002 }.write(&handle)?;
        UnknownB078 { value: 0x000000a0 }.write(&handle)?;
        Unknown00C0 { value: 0x00000110 }.write(&handle)?;
        Unknown00C0 { value: 0x00000210 }.write(&handle)?;
        UnknownB120 { value: 0x00000001 }.write(&handle)?;
        UnknownE120 { value: 0x00000000 }.write(&handle)?;
        UnknownB068 { value: 0x00000004 }.write(&handle)?;
        UnknownB07C { value: 0x00000001 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(10));
        UnknownB07C { value: 0x00000003 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_millis(1));
        Unknown00B8 { value: 0x00000401 }.write(&handle)?;
        Unknown00B8 { value: 0x00000409 }.write(&handle)?;
        Unknown0000 { value: 0x4f006442 }.write(&handle)?;
        Unknown0000 { value: 0x4f00644a }.write(&handle)?;
        UnknownB080 { value: 0x00000077 }.write(&handle)?;
        UnknownB084 { value: 0x0000000f }.write(&handle)?;
        UnknownB088 { value: 0x00000037 }.write(&handle)?;
        UnknownB08C { value: 0x00000037 }.write(&handle)?;
        UnknownB090 { value: 0x000000df }.write(&handle)?;
        UnknownB094 { value: 0x00000057 }.write(&handle)?;
        UnknownB098 { value: 0x00000037 }.write(&handle)?;
        UnknownB09C { value: 0x00000067 }.write(&handle)?;
        UnknownB0A0 { value: 0x00000037 }.write(&handle)?;
        UnknownB0A4 { value: 0x0000002f }.write(&handle)?;
        UnknownB0AC { value: 0x00000028 }.write(&handle)?;
        UnknownB0CC { value: 0x00000001 }.write(&handle)?;
        MipiControl { value: 0x000002f8 }.write(&handle)?;
        UnknownB004 { value: 0x0000008a }.write(&handle)?;
        UnknownB01C { value: 0x00000030 }.write(&handle)?;
        MipiPacketSize { value: 0x00002000 }.write(&handle)?;
        UnknownB02C { value: 0x000000ff }.write(&handle)?;
        MipiFrameBlanking { value: 0x00003e80 }.write(&handle)?;
        MipiFramePeriod { value: 0x00000fa0 }.write(&handle)?;
        UnknownA000 { value: 0x000b0501 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownA008 { value: 0x00002405 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownA004 { value: 0x000b0501 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownA020 { value: 0x00000150 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownB040 { value: 0x00000007 }.write(&handle)?;
        UnknownB064 { value: 0x00000006 }.write(&handle)?;
        UnknownB040 { value: 0x0000000f }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(100));
        UnknownB004 { value: 0x0000008a }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownB0C8 { value: 0x00000003 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_micros(200));
        UnknownB044 { value: 0x00000001 }.write(&handle)?;
        MipiControl { value: 0x000002f9 }.write(&handle)?;
        Unknown7008 { value: 0x00000001 }.write(&handle)?;
        EdfPipelineControl { value: 0x00070001 }.write(&handle)?;
        Unknown8000 { value: 0x0001e085 }.write(&handle)?;
        TimeBaseCtrl { value: 0x00000644 }.write(&handle)?;
        RoiCtrl {
            reserved_0_1: 0,
            td_enable: 1,
            reserved_2_5: 0,
            td_shadow_trigger: 0,
            td_roni_n_en: 1,
            reserved_7_10: 0,
            td_rstn: 0,
            reserved_11_32: 0x1e000a,
        }
        .write(&handle)?;
        Spare0 { value: 0x00000200 }.write(&handle)?;
        BiasDiff {
            idac_ctl: 0x4d,
            vdac_ctl: 0x50,
            buf_stg: 1,
            ibtype_sel: 0,
            mux_sel: 0,
            mux_en: 1,
            vdac_en: 0,
            buf_en: 1,
            idac_en: 1,
            reserved: 0,
            single: 1,
        }
        .write(&handle)?;
        RoFsmCtrl { value: 0x00000000 }.write(&handle)?;
        std::thread::sleep(std::time::Duration::from_millis(1));
        ReadoutCtrl { value: 0x00000200 }.write(&handle)?;
        // }

        let _ = AdcControl::default().read(&handle)?;
        AdcControl { value: 0x00007641 }.write(&handle)?;
        let _ = AdcControl::default().read(&handle)?;
        AdcControl { value: 0x00007643 }.write(&handle)?;
        let _ = AdcMiscCtrl::default().read(&handle)?;
        AdcMiscCtrl { value: 0x00000212 }.write(&handle)?;
        let _ = TempCtrl::default().read(&handle)?;
        TempCtrl { value: 0x00200082 }.write(&handle)?;
        let _ = TempCtrl::default().read(&handle)?;
        TempCtrl { value: 0x00200083 }.write(&handle)?;
        let _ = AdcControl::default().read(&handle)?;
        AdcControl { value: 0x00007641 }.write(&handle)?;
        let _ = IphMirrCtrl::default().read(&handle)?;
        IphMirrCtrl { value: 0x00000003 }.write(&handle)?;
        let _ = IphMirrCtrl::default().read(&handle)?;
        IphMirrCtrl { value: 0x00000003 }.write(&handle)?;
        let _ = LifoCtrl::default().read(&handle)?;
        LifoCtrl { value: 0x00000001 }.write(&handle)?;
        let _ = LifoCtrl::default().read(&handle)?;
        LifoCtrl { value: 0x00000003 }.write(&handle)?;
        let _ = LifoCtrl::default().read(&handle)?;
        LifoCtrl { value: 0x00000007 }.write(&handle)?;
        let _ = ErcReserved6000::default().read(&handle)?;
        ErcReserved6000 { value: 0x00155400 }.write(&handle)?;
        let _ = InDropRateControl::default().read(&handle)?;
        InDropRateControl { value: 0x00000001 }.write(&handle)?;
        let _ = ReferencePeriod::default().read(&handle)?;
        ReferencePeriod { value: 0x000000c8 }.write(&handle)?;
        let _ = TdTargetEventRate::default().read(&handle)?;
        TdTargetEventRate { value: 0x00000fa0 }.write(&handle)?;
        let _ = ErcEnable::default().read(&handle)?;
        ErcEnable { value: 0x00000003 }.write(&handle)?;

        // erc
        let _ = ErcReserved602C::default().read(&handle)?;
        ErcReserved602C { value: 0x00000001 }.write(&handle)?;
        for offset in 0..230 {
            let register = ErcReserved6800 { value: 0x08080808 }.offset(offset);
            let _ = register.read(&handle)?;
            register.write(&handle)?;
        }
        let _ = ErcReserved602C::default().read(&handle)?;
        ErcReserved602C { value: 0x00000002 }.write(&handle)?;

        // t_drop_lut
        for offset in 0..256 {
            let register = TDropLut {
                value: ((offset * 2 + 1) << 16) | (offset * 2),
            };
            let _ = register.read(&handle)?;
            register.write(&handle)?;
        }

        let _ = TDroppingControl::default().read(&handle)?;
        TDroppingControl { value: 0x00000000 }.write(&handle)?;
        let _ = HDroppingControl::default().read(&handle)?;
        HDroppingControl { value: 0x00000000 }.write(&handle)?;
        let _ = VDroppingControl::default().read(&handle)?;
        VDroppingControl { value: 0x00000000 }.write(&handle)?;
        let _ = ErcReserved6000::default().read(&handle)?;
        ErcReserved6000 { value: 0x00155401 }.write(&handle)?;
        let _ = TDroppingControl::default().read(&handle)?;
        TDroppingControl { value: 0x00000000 }.write(&handle)?;
        TdTargetEventRate { value: 0x00000fa0 }.write(&handle)?;
        let _ = EdfReserved7004::default().read(&handle)?;
        EdfReserved7004 {
            reserved_0_10: 0b0111111111,
            external_trigger: if configuration.enable_external_trigger {
                1
            } else {
                0
            },
            reserved_11_32: 0b11000,
        }
        .write(&handle)?;
        loop {
            let mut buffer = vec![0u8; Self::DEFAULT_USB_CONFIGURATION.buffer_size];
            match handle.read_bulk(0x81, &mut buffer, TIMEOUT) {
                Ok(size) => {
                    if size == 0 {
                        break;
                    }
                }
                Err(error) => match error {
                    rusb::Error::Timeout => break,
                    error => return Err(error.into()),
                },
            }
        }
        request(
            &handle,
            &[0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            TIMEOUT,
        )?;
        update_configuration(&handle, None, &configuration)?;
        let _ = ReferencePeriod::default().read(&handle)?;
        let _ = TdTargetEventRate::default().read(&handle)?;
        let _ = ErcReserved6000::default().read(&handle)?;
        let _ = ErcReserved6000::default().read(&handle)?;
        let _ = TDroppingControl::default().read(&handle)?;

        // issd_evk3_imx636_start in hal_psee_plugins/include/devices/imx636/imx636_evk3_issd.h {
        MipiControl { value: 0x000002f9 }.write(&handle)?;
        RoCtrl { value: 0x00000000 }.write(&handle)?;
        let _ = TimeBaseCtrl::default().read(&handle)?;
        TimeBaseCtrl { value: 0x00000645 }.write(&handle)?;
        Unknown002C { value: 0x0022c724 }.write(&handle)?;
        RoiCtrl {
            reserved_0_1: 0,
            td_enable: 1,
            reserved_2_5: 0,
            td_shadow_trigger: 0,
            td_roni_n_en: (!configuration.invert_mask) as u32,
            reserved_7_10: 0,
            td_rstn: 1,
            reserved_11_32: 0x1e000a,
        }
        .write(&handle)?;
        // }

        let handle = std::sync::Arc::new(handle);
        let ring_error_flag = error_flag.clone();
        Ok(Device {
            handle: handle.clone(),
            ring: usb::Ring::new(
                handle.clone(),
                usb_configuration,
                move |usb_error| {
                    ring_error_flag.store_if_not_set(Self::Error::from(usb_error));
                },
                event_loop,
                usb::TransferType::Bulk {
                    endpoint: 1 | libusb1_sys::constants::LIBUSB_ENDPOINT_IN,
                    timeout: std::time::Duration::from_millis(100),
                },
            )?,
            configuration_updater: configuration::Updater::new(
                configuration,
                ConfigurationUpdaterContext { handle, error_flag },
                |context, previous_configuration, configuration| {
                    if let Err(error) = update_configuration(
                        &context.handle,
                        Some(previous_configuration),
                        configuration,
                    ) {
                        context.error_flag.store_if_not_set(error);
                    }
                    context
                },
            ),
            serial,
        })
    }

    fn next_with_timeout(&mut self, timeout: &std::time::Duration) -> Option<usb::BufferView> {
        self.ring.next_with_timeout(timeout)
    }

    fn serial(&self) -> String {
        self.serial.clone()
    }

    fn speed(&self) -> usb::Speed {
        self.handle.device().speed().into()
    }

    fn adapter(&self) -> Self::Adapter {
        Self::Adapter::from_dimensions(Self::PROPERTIES.width, Self::PROPERTIES.height)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // issd_evk3_imx636_stop in hal_psee_plugins/include/devices/imx636/imx636_evk3_issd.h {
        let _ = RoiCtrl {
            reserved_0_1: 0,
            td_enable: 1,
            reserved_2_5: 0,
            td_shadow_trigger: 0,
            td_roni_n_en: 1,
            reserved_7_10: 0,
            td_rstn: 0,
            reserved_11_32: 0x1e000a,
        }
        .write(&self.handle);
        let _ = Unknown002C { value: 0x0022c324 }.write(&self.handle);
        let _ = RoCtrl { value: 0x00000002 }.write(&self.handle);
        let _ = std::thread::sleep(std::time::Duration::from_millis(1));
        let _ = TimeBaseCtrl::default().read(&self.handle);
        let _ = TimeBaseCtrl { value: 0x00000644 }.write(&self.handle);
        let _ = MipiControl { value: 0x000002f8 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(300));
        // }

        // issd_evk3_imx636_destroy in hal_psee_plugins/include/devices/imx636/imx636_evk3_issd.h {
        let _ = Unknown0070 { value: 0x00400008 }.write(&self.handle);
        let _ = Unknown006C { value: 0x0ee47114 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(500));
        let _ = UnknownA00C { value: 0x00020400 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(500));
        let _ = UnknownA010 { value: 0x00008068 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(200));
        let _ = Unknown1104 { value: 0x00000000 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(200));
        let _ = UnknownA020 { value: 0x00000050 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(200));
        let _ = UnknownA004 { value: 0x000b0500 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(200));
        let _ = UnknownA008 { value: 0x00002404 }.write(&self.handle);
        std::thread::sleep(std::time::Duration::from_micros(200));
        let _ = UnknownA000 { value: 0x000b0500 }.write(&self.handle);
        let _ = UnknownB044 { value: 0x00000000 }.write(&self.handle);
        let _ = UnknownB004 { value: 0x0000000a }.write(&self.handle);
        let _ = UnknownB040 { value: 0x0000000e }.write(&self.handle);
        let _ = UnknownB0C8 { value: 0x00000000 }.write(&self.handle);
        let _ = UnknownB040 { value: 0x00000006 }.write(&self.handle);
        let _ = UnknownB040 { value: 0x00000004 }.write(&self.handle);
        let _ = Unknown0000 { value: 0x4f006442 }.write(&self.handle);
        let _ = Unknown0000 { value: 0x0f006442 }.write(&self.handle);
        let _ = Unknown00B8 { value: 0x00000401 }.write(&self.handle);
        let _ = Unknown00B8 { value: 0x00000400 }.write(&self.handle);
        let _ = UnknownB07C { value: 0x00000000 }.write(&self.handle);
        // }
    }
}

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

fn request(
    handle: &rusb::DeviceHandle<rusb::Context>,
    buffer: &[u8],
    timeout: std::time::Duration,
) -> Result<Vec<u8>, Error> {
    let written = handle.write_bulk(0x02, buffer, timeout)?;
    if buffer.len() != written {
        return Err(Error::ShortWrite {
            requested: buffer.len(),
            written,
        });
    }
    let mut buffer = vec![0; 1024];
    let read = handle.read_bulk(0x82, &mut buffer, timeout)?;
    buffer.truncate(read);
    Ok(buffer)
}

macro_rules! update_bias {
    ($name:ident, $register:ident, $handle:ident, $previous_biases:ident, $biases:expr) => {
        if match $previous_biases {
            Some(previous_biases) => previous_biases.$name != $biases.$name,
            None => true,
        } {
            $register {
                idac_ctl: $biases.$name as u32,
                vdac_ctl: 0,
                buf_stg: 1,
                ibtype_sel: 0,
                mux_sel: 0,
                mux_en: 1,
                vdac_en: 0,
                buf_en: 1,
                idac_en: 1,
                reserved: 0,
                single: 1,
            }
            .write($handle)?;
        }
    };
}

fn update_configuration(
    handle: &rusb::DeviceHandle<rusb::Context>,
    previous_configuration: Option<&Configuration>,
    configuration: &Configuration,
) -> Result<(), Error> {
    {
        let previous_biases = previous_configuration.map(|configuration| &configuration.biases);
        update_bias!(pr, BiasPr, handle, previous_biases, configuration.biases);
        update_bias!(fo, BiasFo, handle, previous_biases, configuration.biases);
        update_bias!(hpf, BiasHpf, handle, previous_biases, configuration.biases);
        update_bias!(
            diff_on,
            BiasDiffOn,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(
            diff,
            BiasDiff,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(
            diff_off,
            BiasDiffOff,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(inv, BiasInv, handle, previous_biases, configuration.biases);
        update_bias!(
            refr,
            BiasRefr,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(
            reqpuy,
            BiasReqpuy,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(
            reqpux,
            BiasReqpux,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(
            sendreqpdy,
            BiasSendreqpdy,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(
            unknown_1,
            BiasUnknown1,
            handle,
            previous_biases,
            configuration.biases
        );
        update_bias!(
            unknown_2,
            BiasUnknown2,
            handle,
            previous_biases,
            configuration.biases
        );
    }
    if match previous_configuration {
        Some(previous_configuration) => {
            previous_configuration.x_mask != configuration.x_mask
                || previous_configuration.y_mask != configuration.y_mask
                || previous_configuration.invert_mask != configuration.invert_mask
        }
        None => true,
    } {
        for offset in 0..((configuration.x_mask.len() as u32) * 2) {
            let register = TdRoiX {
                value: if (offset % 2) == 0 {
                    (configuration.x_mask[(offset / 2) as usize] & 0xffffffffu64) as u32
                } else {
                    ((configuration.x_mask[(offset / 2) as usize] & 0xffffffff00000000u64) >> 32)
                        as u32
                },
            }
            .offset(offset);
            let _ = register.read(&handle)?;
            register.write(&handle)?;
        }
        for offset in 0..((configuration.y_mask.len() as u32) * 2 - 1) {
            let register = TdRoiY {
                value: if (offset % 2) == 0 {
                    let [byte2, byte3, _, _, _, _, _, _] = configuration.y_mask
                        [configuration.y_mask.len() - 1 - (offset / 2) as usize]
                        .to_le_bytes();
                    if offset < (configuration.y_mask.len() as u32) * 2 - 2 {
                        let [_, _, _, _, _, _, byte0, byte1] = configuration.y_mask
                            [configuration.y_mask.len() - 2 - (offset / 2) as usize]
                            .to_le_bytes();
                        u32::from_le_bytes([
                            byte3.reverse_bits(),
                            byte2.reverse_bits(),
                            byte1.reverse_bits(),
                            byte0.reverse_bits(),
                        ])
                    } else {
                        u32::from_le_bytes([byte3, byte2, 0xff, 0x00])
                    }
                } else {
                    let [_, _, byte0, byte1, byte2, byte3, _, _] = configuration.y_mask
                        [configuration.y_mask.len() - 2 - (offset / 2) as usize]
                        .to_le_bytes();
                    u32::from_le_bytes([
                        byte3.reverse_bits(),
                        byte2.reverse_bits(),
                        byte1.reverse_bits(),
                        byte0.reverse_bits(),
                    ])
                },
            }
            .offset(offset);
            let _ = register.read(&handle)?;
            register.write(&handle)?;
        }
        RoiCtrl {
            reserved_0_1: 0,
            td_enable: 1,
            reserved_2_5: 0,
            td_shadow_trigger: 1,
            td_roni_n_en: configuration.invert_mask as u32,
            reserved_7_10: 0,
            td_rstn: previous_configuration.is_some() as u32,
            reserved_11_32: 0x1e000a,
        }
        .write(&handle)?;
    }
    Ok(())
}

struct ConfigurationUpdaterContext<IntoError>
where
    IntoError: From<Error> + Clone + Send,
{
    handle: std::sync::Arc<rusb::DeviceHandle<rusb::Context>>,
    error_flag: error::Flag<IntoError>,
}

struct RuntimeRegister {
    address: u32,
    value: u32,
}

trait Register {
    fn address(&self) -> u32;

    fn value(&self) -> u32;

    fn offset(&self, registers: u32) -> RuntimeRegister;

    fn read(&self, handle: &rusb::DeviceHandle<rusb::Context>) -> Result<u32, Error> {
        let address = self.address();
        let buffer = [
            0x02,
            0x01,
            0x01,
            0x00,
            0x0c,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            (address & 0xff) as u8,
            ((address >> 8) & 0xff) as u8,
            ((address >> 16) & 0xff) as u8,
            ((address >> 24) & 0xff) as u8,
            0x01,
            0x00,
            0x00,
            0x00,
        ];
        let result = request(handle, &buffer, std::time::Duration::from_millis(1000))?;
        if result.len() != buffer.len() {
            return Err(Error::RegisterReadShortResponse(address));
        }
        if result[0..16] != buffer[0..16] {
            return Err(Error::RegisterReadMismatch(address));
        }
        // unwrap: slice has the right number of bytes
        Ok(u32::from_le_bytes(result[16..20].try_into().unwrap()))
    }

    fn write(&self, handle: &rusb::DeviceHandle<rusb::Context>) -> Result<(), Error> {
        let address = self.address();
        let value = self.value();
        request(
            handle,
            &[
                0x02,
                0x01,
                0x01,
                0x40,
                0x0c,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                (address & 0xff) as u8,
                ((address >> 8) & 0xff) as u8,
                ((address >> 16) & 0xff) as u8,
                ((address >> 24) & 0xff) as u8,
                (value & 0xff) as u8,
                ((value >> 8) & 0xff) as u8,
                ((value >> 16) & 0xff) as u8,
                ((value >> 24) & 0xff) as u8,
            ],
            std::time::Duration::from_millis(1000),
        )?;
        Ok(())
    }
}

impl Register for RuntimeRegister {
    fn address(&self) -> u32 {
        self.address
    }
    fn value(&self) -> u32 {
        self.value
    }
    fn offset(&self, registers: u32) -> RuntimeRegister {
        RuntimeRegister {
            address: self.address + registers * 4,
            value: self.value,
        }
    }
}

macro_rules! register {
    ($name:ident, $address:literal, {$($subname:ident: $substart:literal..$subend:literal),+ $(,)?}) => {
        #[derive(Default)]
        struct $name {
            $(
                $subname: u32,
            )+
        }
        $(
            const _: () = assert!($substart < $subend);
        )+
        impl Register for $name {
            fn address(&self) -> u32 {
                $address
            }
            fn value(&self) -> u32 {
                0u32
                $(
                    | ((self.$subname & (((1u64 << ($subend - $substart)) - 1) as u32)) << $substart)
                )+
            }
            fn offset(&self, registers: u32) -> RuntimeRegister {
                RuntimeRegister  {
                    address: $address + registers * 4,
                    value: self.value(),
                }
            }
        }
    };
}

register! { Unknown0000, 0x0000, { value: 0..32 } }
register! { RoiCtrl, 0x0004, {
    reserved_0_1: 0..1,
    td_enable: 1..2,
    reserved_2_5: 2..5,
    td_shadow_trigger: 5..6,
    td_roni_n_en: 6..7,
    reserved_7_10: 7..10,
    td_rstn: 10..11,
    reserved_11_32: 11..32,
} }
register! { LifoCtrl, 0x000C, { value: 0..32 } }
register! { LifoStatus, 0x0010, { value: 0..32 } }
register! { Reserved0014, 0x0014, { value: 0..32 } }
register! { Spare0, 0x0018, { value: 0..32 } }
register! { Unknown001C, 0x001C, { value: 0..32 } }
register! { RefractoryCtrl, 0x0020, { value: 0..32 } }
register! { Unknown002C, 0x002C, { value: 0..32 } }
register! { RoiWinCtrl, 0x0034, { value: 0..32 } }
register! { RoiWinStartAddr, 0x0038, { value: 0..32 } }
register! { RoiWinEndAddr, 0x003C, { value: 0..32 } }
register! { DigPad2Ctrl, 0x0044, { value: 0..32 } }
register! { AdcControl, 0x004C, { value: 0..32 } }
register! { AdcStatus, 0x0050, { value: 0..32 } }
register! { AdcMiscCtrl, 0x0054, { value: 0..32 } }
register! { TempCtrl, 0x005C, { value: 0..32 } }
register! { Unknown006C, 0x006C, { value: 0..32 } }
register! { Unknown0070, 0x0070, { value: 0..32 } }
register! { IphMirrCtrl, 0x0074, { value: 0..32 } }
register! { GcdCtrl1, 0x0078, { value: 0..32 } }
register! { GcdShadowCtrl, 0x0090, { value: 0..32 } }
register! { GcdShadowStatus, 0x0094, { value: 0..32 } }
register! { GcdShadowCounter, 0x0098, { value: 0..32 } }
register! { Unknown00B8, 0x00B8, { value: 0..32 } }
register! { Unknown00C0, 0x00C0, { value: 0..32 } }
register! { StopSequenceControl, 0x00C8, { value: 0..32 } }
register! { BiasPr, 0x1000, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasFo, 0x1004, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasHpf, 0x100C, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasDiffOn, 0x1010, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasDiff, 0x1014, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasDiffOff, 0x1018, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasInv, 0x101C, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasRefr, 0x1020, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasReqpuy, 0x1040, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasReqpux, 0x1044, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasSendreqpdy, 0x1048, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasUnknown1, 0x104C, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BiasUnknown2, 0x1050, {
    idac_ctl: 0..8,
    vdac_ctl: 8..16,
    buf_stg: 16..19,
    ibtype_sel: 19..20,
    mux_sel: 20..21,
    mux_en: 21..22,
    vdac_en: 22..23,
    buf_en: 23..24,
    idac_en: 24..25,
    reserved: 25..28,
    single: 28..29,
} }
register! { BgenCtrl, 0x1100, { value: 0..32 } }
register! { Unknown1104, 0x1104, { value: 0..32 } }
register! { TdRoiX, 0x2000, { value: 0..32 } }
register! { TdRoiY, 0x4000, { value: 0..32 } }
register! { ErcReserved6000, 0x6000, { value: 0..32 } }
register! { InDropRateControl, 0x6004, { value: 0..32 } }
register! { ReferencePeriod, 0x6008, { value: 0..32 } }
register! { TdTargetEventRate, 0x600C, { value: 0..32 } }
register! { ErcEnable, 0x6028, { value: 0..32 } }
register! { ErcReserved602C, 0x602C, { value: 0..32 } }
register! { TDroppingControl, 0x6050, { value: 0..32 } }
register! { HDroppingControl, 0x6060, { value: 0..32 } }
register! { VDroppingControl, 0x6070, { value: 0..32 } }
register! { TDropLut, 0x6400, { value: 0..32 } }
register! { ErcReserved6800, 0x6800, { value: 0..32 } }
register! { EdfPipelineControl, 0x7000, { value: 0..32 } }
register! { EdfReserved7004, 0x7004, {
    reserved_0_10: 0..10,
    external_trigger: 10..11,
    reserved_11_32: 11..32,
} }
register! { Unknown7008, 0x7008, { value: 0..32 } }
register! { Unknown8000, 0x8000, { value: 0..32 } }
register! { ReadoutCtrl, 0x9000, { value: 0..32 } }
register! { RoFsmCtrl, 0x9004, { value: 0..32 } }
register! { TimeBaseCtrl, 0x9008, { value: 0..32 } }
register! { DigCtrl, 0x900C, { value: 0..32 } }
register! { DigStartPos, 0x9010, { value: 0..32 } }
register! { DigEndPos, 0x9014, { value: 0..32 } }
register! { RoCtrl, 0x9028, { value: 0..32 } }
register! { AreaX0Addr, 0x902C, { value: 0..32 } }
register! { AreaX1Addr, 0x9030, { value: 0..32 } }
register! { AreaX2Addr, 0x9034, { value: 0..32 } }
register! { AreaX3Addr, 0x9038, { value: 0..32 } }
register! { AreaX4Addr, 0x903C, { value: 0..32 } }
register! { AreaY0Addr, 0x9040, { value: 0..32 } }
register! { AreaY1Addr, 0x9044, { value: 0..32 } }
register! { AreaY2Addr, 0x9048, { value: 0..32 } }
register! { AreaY3Addr, 0x904C, { value: 0..32 } }
register! { AreaY4Addr, 0x9050, { value: 0..32 } }
register! { CounterCtrl, 0x9054, { value: 0..32 } }
register! { CounterTimerThreshold, 0x9058, { value: 0..32 } }
register! { DigitalMaskPixel00, 0x9100, { value: 0..32 } }
register! { DigitalMaskPixel01, 0x9104, { value: 0..32 } }
register! { DigitalMaskPixel02, 0x9108, { value: 0..32 } }
register! { DigitalMaskPixel03, 0x910C, { value: 0..32 } }
register! { DigitalMaskPixel04, 0x9110, { value: 0..32 } }
register! { DigitalMaskPixel05, 0x9114, { value: 0..32 } }
register! { DigitalMaskPixel06, 0x9118, { value: 0..32 } }
register! { DigitalMaskPixel07, 0x911C, { value: 0..32 } }
register! { DigitalMaskPixel08, 0x9120, { value: 0..32 } }
register! { DigitalMaskPixel09, 0x9124, { value: 0..32 } }
register! { DigitalMaskPixel10, 0x9128, { value: 0..32 } }
register! { DigitalMaskPixel11, 0x912C, { value: 0..32 } }
register! { DigitalMaskPixel12, 0x9130, { value: 0..32 } }
register! { DigitalMaskPixel13, 0x9134, { value: 0..32 } }
register! { DigitalMaskPixel14, 0x9138, { value: 0..32 } }
register! { DigitalMaskPixel15, 0x913C, { value: 0..32 } }
register! { DigitalMaskPixel16, 0x9140, { value: 0..32 } }
register! { DigitalMaskPixel17, 0x9144, { value: 0..32 } }
register! { DigitalMaskPixel18, 0x9148, { value: 0..32 } }
register! { DigitalMaskPixel19, 0x914C, { value: 0..32 } }
register! { DigitalMaskPixel20, 0x9150, { value: 0..32 } }
register! { DigitalMaskPixel21, 0x9154, { value: 0..32 } }
register! { DigitalMaskPixel22, 0x9158, { value: 0..32 } }
register! { DigitalMaskPixel23, 0x915C, { value: 0..32 } }
register! { DigitalMaskPixel24, 0x9160, { value: 0..32 } }
register! { DigitalMaskPixel25, 0x9164, { value: 0..32 } }
register! { DigitalMaskPixel26, 0x9168, { value: 0..32 } }
register! { DigitalMaskPixel27, 0x916C, { value: 0..32 } }
register! { DigitalMaskPixel28, 0x9170, { value: 0..32 } }
register! { DigitalMaskPixel29, 0x9174, { value: 0..32 } }
register! { DigitalMaskPixel30, 0x9178, { value: 0..32 } }
register! { DigitalMaskPixel31, 0x917C, { value: 0..32 } }
register! { DigitalMaskPixel32, 0x9180, { value: 0..32 } }
register! { DigitalMaskPixel33, 0x9184, { value: 0..32 } }
register! { DigitalMaskPixel34, 0x9188, { value: 0..32 } }
register! { DigitalMaskPixel35, 0x918C, { value: 0..32 } }
register! { DigitalMaskPixel36, 0x9190, { value: 0..32 } }
register! { DigitalMaskPixel37, 0x9194, { value: 0..32 } }
register! { DigitalMaskPixel38, 0x9198, { value: 0..32 } }
register! { DigitalMaskPixel39, 0x919C, { value: 0..32 } }
register! { DigitalMaskPixel40, 0x91A0, { value: 0..32 } }
register! { DigitalMaskPixel41, 0x91A4, { value: 0..32 } }
register! { DigitalMaskPixel42, 0x91A8, { value: 0..32 } }
register! { DigitalMaskPixel43, 0x91AC, { value: 0..32 } }
register! { DigitalMaskPixel44, 0x91B0, { value: 0..32 } }
register! { DigitalMaskPixel45, 0x91B4, { value: 0..32 } }
register! { DigitalMaskPixel46, 0x91B8, { value: 0..32 } }
register! { DigitalMaskPixel47, 0x91BC, { value: 0..32 } }
register! { DigitalMaskPixel48, 0x91C0, { value: 0..32 } }
register! { DigitalMaskPixel49, 0x91C4, { value: 0..32 } }
register! { DigitalMaskPixel50, 0x91C8, { value: 0..32 } }
register! { DigitalMaskPixel51, 0x91CC, { value: 0..32 } }
register! { DigitalMaskPixel52, 0x91D0, { value: 0..32 } }
register! { DigitalMaskPixel53, 0x91D4, { value: 0..32 } }
register! { DigitalMaskPixel54, 0x91D8, { value: 0..32 } }
register! { DigitalMaskPixel55, 0x91DC, { value: 0..32 } }
register! { DigitalMaskPixel56, 0x91E0, { value: 0..32 } }
register! { DigitalMaskPixel57, 0x91E4, { value: 0..32 } }
register! { DigitalMaskPixel58, 0x91E8, { value: 0..32 } }
register! { DigitalMaskPixel59, 0x91EC, { value: 0..32 } }
register! { DigitalMaskPixel60, 0x91F0, { value: 0..32 } }
register! { DigitalMaskPixel61, 0x91F4, { value: 0..32 } }
register! { DigitalMaskPixel62, 0x91F8, { value: 0..32 } }
register! { DigitalMaskPixel63, 0x91FC, { value: 0..32 } }
register! { AreaCnt00, 0x9200, { value: 0..32 } }
register! { AreaCnt01, 0x9204, { value: 0..32 } }
register! { AreaCnt02, 0x9208, { value: 0..32 } }
register! { AreaCnt03, 0x920C, { value: 0..32 } }
register! { AreaCnt04, 0x9210, { value: 0..32 } }
register! { AreaCnt05, 0x9214, { value: 0..32 } }
register! { AreaCnt06, 0x9218, { value: 0..32 } }
register! { AreaCnt07, 0x921C, { value: 0..32 } }
register! { AreaCnt08, 0x9220, { value: 0..32 } }
register! { AreaCnt09, 0x9224, { value: 0..32 } }
register! { AreaCnt10, 0x9228, { value: 0..32 } }
register! { AreaCnt11, 0x922C, { value: 0..32 } }
register! { AreaCnt12, 0x9230, { value: 0..32 } }
register! { AreaCnt13, 0x9234, { value: 0..32 } }
register! { AreaCnt14, 0x9238, { value: 0..32 } }
register! { AreaCnt15, 0x923C, { value: 0..32 } }
register! { EvtVectorCntVal, 0x9244, { value: 0..32 } }
register! { UnknownA000, 0xA000, { value: 0..32 } }
register! { UnknownA004, 0xA004, { value: 0..32 } }
register! { UnknownA008, 0xA008, { value: 0..32 } }
register! { UnknownA00C, 0xA00C, { value: 0..32 } }
register! { UnknownA010, 0xA010, { value: 0..32 } }
register! { UnknownA020, 0xA020, { value: 0..32 } }
register! { MipiControl, 0xB000, { value: 0..32 } }
register! { UnknownB004, 0xB004, { value: 0..32 } }
register! { UnknownB01C, 0xB01C, { value: 0..32 } }
register! { MipiPacketSize, 0xB020, { value: 0..32 } }
register! { MipiPacketTimeout, 0xB024, { value: 0..32 } }
register! { MipiFramePeriod, 0xB028, { value: 0..32 } }
register! { UnknownB02C, 0xB02C, { value: 0..32 } }
register! { MipiFrameBlanking, 0xB030, { value: 0..32 } }
register! { UnknownB040, 0xB040, { value: 0..32 } }
register! { UnknownB044, 0xB044, { value: 0..32 } }
register! { UnknownB064, 0xB064, { value: 0..32 } }
register! { UnknownB068, 0xB068, { value: 0..32 } }
register! { UnknownB074, 0xB074, { value: 0..32 } }
register! { UnknownB078, 0xB078, { value: 0..32 } }
register! { UnknownB07C, 0xB07C, { value: 0..32 } }
register! { UnknownB080, 0xB080, { value: 0..32 } }
register! { UnknownB084, 0xB084, { value: 0..32 } }
register! { UnknownB088, 0xB088, { value: 0..32 } }
register! { UnknownB08C, 0xB08C, { value: 0..32 } }
register! { UnknownB090, 0xB090, { value: 0..32 } }
register! { UnknownB094, 0xB094, { value: 0..32 } }
register! { UnknownB098, 0xB098, { value: 0..32 } }
register! { UnknownB09C, 0xB09C, { value: 0..32 } }
register! { UnknownB0A0, 0xB0A0, { value: 0..32 } }
register! { UnknownB0A4, 0xB0A4, { value: 0..32 } }
register! { UnknownB0AC, 0xB0AC, { value: 0..32 } }
register! { UnknownB0C8, 0xB0C8, { value: 0..32 } }
register! { UnknownB0CC, 0xB0CC, { value: 0..32 } }
register! { UnknownB120, 0xB120, { value: 0..32 } }
register! { AfkPipelineControl, 0xC000, { value: 0..32 } }
register! { ReservedC004, 0xC004, { value: 0..32 } }
register! { FilterPeriod, 0xC008, { value: 0..32 } }
register! { Invalidation, 0xC0C0, { value: 0..32 } }
register! { AfkInitialization, 0xC0C4, { value: 0..32 } }
register! { StcPipelineControl, 0xD000, { value: 0..32 } }
register! { StcParam, 0xD004, { value: 0..32 } }
register! { TrailParam, 0xD008, { value: 0..32 } }
register! { Timestamping, 0xD00C, { value: 0..32 } }
register! { ReservedD0c0, 0xD0C0, { value: 0..32 } }
register! { StcInitialization, 0xD0C4, { value: 0..32 } }
register! { SlvsControl, 0xE000, { value: 0..32 } }
register! { SlvsPacketSize, 0xE020, { value: 0..32 } }
register! { SlvsPacketTimeout, 0xE024, { value: 0..32 } }
register! { SlvsFrameBlanking, 0xE030, { value: 0..32 } }
register! { UnknownE120, 0xE120, { value: 0..32 } }
register! { SlvsPhyLogicCtrl00, 0xE150, { value: 0..32 } }
register! { Reset, 0x400004, { value: 0..32 } }
