#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use esp32_hal::prelude::*;

use esp32_hal::clock_control::{sleep, CPUSource, ClockControl};
use esp32_hal::dport::Split;
use esp32_hal::dprintln;
use esp32_hal::serial::{config::Config, NoRx, NoTx, Serial};
const BLINK_HZ: Hertz = Hertz(1);

#[no_mangle]
fn main() -> ! {
    let dp = unsafe { esp32::Peripherals::steal() };

    let mut timg0 = dp.TIMG0;
    let mut timg1 = dp.TIMG1;

    let (mut dport, dport_clock_control) = dp.DPORT.split();

    // (https://github.com/espressif/openocd-esp32/blob/97ba3a6bb9eaa898d91df923bbedddfeaaaf28c9/src/target/esp32.c#L431)
    // openocd disables the watchdog timers on halt
    // we will do it manually on startup
    //    disable_timg_wdts(&mut timg0, &mut timg1);

    disable_timg_wdts(&mut timg0, &mut timg1);

    // setup clocks & watchdog
    let mut clock_control =
        ClockControl::new(dp.RTCCNTL, dp.APB_CTRL, dport_clock_control).unwrap();

    // BUG: why does setting clock to Xtal not work?
    clock_control
        .set_cpu_frequencies(
            CPUSource::Xtal,
            40.MHz(),
            CPUSource::PLL,
            80.MHz(),
            CPUSource::PLL,
            240.MHz(),
        )
        .unwrap();
    //clock_control.set_cpu_frequency_to_xtal(26.MHz()).unwrap();
    let (clock_control_config, mut watchdog) = clock_control.freeze().unwrap();

    watchdog.start(100.s());

    // setup serial controller
    let mut uart0 = Serial::uart0(
        dp.UART0,
        (NoTx, NoRx),
        Config::default(),
        clock_control_config,
        &mut dport,
    )
    .unwrap();

    uart0.change_baudrate(115200).unwrap();

    let (mut tx, _rx) = uart0.split();

    // print startup message
    writeln!(tx, "\n\nReboot!\n").unwrap();

    writeln!(tx, "Running on core {:0x}\n", xtensa_lx6_rt::get_core_id()).unwrap();

    sleep(100.ms());
    writeln!(tx, "{:?}\n", clock_control_config).unwrap();

    sleep(100.ms());

    writeln!(tx, "{:?}\n", watchdog.config().unwrap()).unwrap();

    /*  clock_control_config
        .add_callback(&|| dprintln!("callback"))
        .unwrap();

    let lock = clock_control_config.lock_cpu_frequency();

    drop(lock);*/

    // uncomment next line to test panic exit
    // panic!("panic test");

    let mut x: u32 = 0;
    let mut prev_ccount = 0;

    loop {
        for j in 0..2 {
            let apb_guard = if j == 0 {
                Some(clock_control_config.lock_apb_frequency())
            } else {
                None
            };

            for i in 0..2 {
                let cpu_guard = if i == 0 {
                    Some(clock_control_config.lock_cpu_frequency())
                } else {
                    None
                };

                x = x.wrapping_add(1);

                let ccount = xtensa_lx6_rt::get_cycle_count();
                let ccount_diff = ccount.wrapping_sub(prev_ccount);

                writeln!(
                    tx,
                    "Loop: {}, cycles: {}, cycles since previous {}, CPU: {}, PLL: {}, APB: {}",
                    x,
                    ccount,
                    ccount_diff,
                    clock_control_config.cpu_frequency(),
                    clock_control_config.pll_frequency(),
                    clock_control_config.apb_frequency()
                )
                .unwrap();

                prev_ccount = ccount;

                sleep((Hertz(1_000_000) / BLINK_HZ).us());

                // comment out next line to check watchdog behavior
                watchdog.feed();

                if cpu_guard.is_some() {
                    drop(cpu_guard.unwrap())
                }
            }
            if apb_guard.is_some() {
                drop(apb_guard.unwrap())
            }
        }
    }
}

const WDT_WKEY_VALUE: u32 = 0x50D83AA1;

fn disable_timg_wdts(timg0: &mut esp32::TIMG0, timg1: &mut esp32::TIMG1) {
    timg0
        .wdtwprotect
        .write(|w| unsafe { w.bits(WDT_WKEY_VALUE) });
    timg1
        .wdtwprotect
        .write(|w| unsafe { w.bits(WDT_WKEY_VALUE) });

    timg0.wdtconfig0.write(|w| unsafe { w.bits(0x0) });
    timg1.wdtconfig0.write(|w| unsafe { w.bits(0x0) });
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    dprintln!("\n\n*** {:?}", info);
    loop {}
}
