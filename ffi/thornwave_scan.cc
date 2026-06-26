#include "thornwave_scan.h"

#include <powermon.h>
#include <powermon_scanner.h>

#include <algorithm>
#include <cstring>
#include <exception>
#include <mutex>
#include <new>
#include <string>

namespace {
thread_local std::string last_error;

void set_last_error(const char *message) {
    last_error = message == nullptr ? "unknown Thornwave scanner error" : message;
}

void set_last_error(const std::exception &error) {
    last_error = error.what();
}

void clear_last_error() {
    last_error.clear();
}

void copy_string(char *dest, size_t dest_size, const std::string &value) {
    if (dest == nullptr || dest_size == 0) {
        return;
    }

    const size_t bytes = std::min(dest_size - 1, value.size());
    std::memcpy(dest, value.data(), bytes);
    dest[bytes] = '\0';
}

bool is_network_revision(uint8_t hardware_revision_bcd) {
    const uint8_t family = hardware_revision_bcd & Powermon::FAMILY_MASK;
    return family == Powermon::POWERMON_E || family == Powermon::POWERMON_W;
}

TwAdvertisement convert_advertisement(const PowermonScanner::Advertisement &input) {
    TwAdvertisement out{};

    out.serial = input.serial;
    out.address_raw = input.address;
    out.device_time_raw = input.time;
    out.flags_raw = input.flags;
    out.voltage1_volts = input.voltage1;
    out.voltage2_volts = input.voltage2;
    out.current_amps = input.current;
    out.power_watts = input.power;
    out.coulomb_meter_raw = input.coulomb_meter;
    out.power_meter_raw = input.power_meter;
    out.temperature_celsius = input.temperature;
    out.firmware_version_bcd = input.firmware_version_bcd;
    out.hardware_revision_bcd = input.hardware_revision_bcd;
    out.power_status_code = static_cast<uint8_t>(input.power_status);
    out.soc_raw = input.soc;
    out.runtime_raw = input.runtime;
    out.rssi_dbm = input.rssi;
    out.temperature_is_external = input.isExternalTemperature();

    copy_string(out.name, sizeof(out.name), input.name);
    copy_string(out.model, sizeof(out.model), Powermon::getHardwareString(input.hardware_revision_bcd));
    copy_string(out.power_status_display, sizeof(out.power_status_display), Powermon::getPowerStatusString(input.power_status));

    if (is_network_revision(input.hardware_revision_bcd)) {
        copy_string(out.address_display, sizeof(out.address_display), Powermon::getIpAddressString(static_cast<uint32_t>(input.address)));
    } else {
        copy_string(out.address_display, sizeof(out.address_display), Powermon::getMacAddressString(input.address));
    }

    return out;
}
} // namespace

struct TwScanner {
    PowermonScanner *scanner = nullptr;
    TwAdvertisementCallback callback = nullptr;
    void *user_data = nullptr;
    std::mutex mutex;
    bool alive = true;
};

extern "C" TwScanner *tw_scanner_create(void) {
    try {
        clear_last_error();
        TwScanner *handle = new TwScanner();
        handle->scanner = PowermonScanner::createInstance();
        if (handle->scanner == nullptr) {
            delete handle;
            set_last_error("PowermonScanner::createInstance returned null");
            return nullptr;
        }
        return handle;
    } catch (const std::exception &error) {
        set_last_error(error);
        return nullptr;
    } catch (...) {
        set_last_error("unknown exception while creating Thornwave scanner");
        return nullptr;
    }
}

extern "C" void tw_scanner_destroy(TwScanner *handle) {
    if (handle == nullptr) {
        return;
    }

    {
        std::lock_guard<std::mutex> lock(handle->mutex);
        handle->alive = false;
        handle->callback = nullptr;
        handle->user_data = nullptr;
    }

    if (handle->scanner != nullptr) {
        try {
            handle->scanner->stopBleScan();
        } catch (...) {
        }
        delete handle->scanner;
        handle->scanner = nullptr;
    }

    delete handle;
}

extern "C" int tw_scanner_set_callback(TwScanner *handle, TwAdvertisementCallback callback, void *user_data) {
    if (handle == nullptr || handle->scanner == nullptr) {
        set_last_error("tw_scanner_set_callback called with null scanner");
        return -1;
    }

    try {
        clear_last_error();
        {
            std::lock_guard<std::mutex> lock(handle->mutex);
            handle->callback = callback;
            handle->user_data = user_data;
        }

        handle->scanner->setCallback([handle](const PowermonScanner::Advertisement &advertisement) {
            TwAdvertisement out = convert_advertisement(advertisement);

            std::lock_guard<std::mutex> lock(handle->mutex);
            if (handle->alive && handle->callback != nullptr) {
                handle->callback(handle->user_data, &out);
            }
        });

        return 0;
    } catch (const std::exception &error) {
        set_last_error(error);
        return -1;
    } catch (...) {
        set_last_error("unknown exception while setting Thornwave scanner callback");
        return -1;
    }
}

extern "C" int tw_scanner_start_ble(TwScanner *handle) {
    if (handle == nullptr || handle->scanner == nullptr) {
        set_last_error("tw_scanner_start_ble called with null scanner");
        return -1;
    }

    try {
        clear_last_error();
        handle->scanner->startBleScan();
        return 0;
    } catch (const std::exception &error) {
        set_last_error(error);
        return -1;
    } catch (...) {
        set_last_error("unknown exception while starting Thornwave BLE scan");
        return -1;
    }
}

extern "C" void tw_scanner_stop_ble(TwScanner *handle) {
    if (handle != nullptr && handle->scanner != nullptr) {
        try {
            handle->scanner->stopBleScan();
        } catch (...) {
        }
    }
}

extern "C" uint16_t tw_library_version_bcd(void) {
    try {
        clear_last_error();
        return Powermon::getVersion();
    } catch (const std::exception &error) {
        set_last_error(error);
        return 0;
    } catch (...) {
        set_last_error("unknown exception while reading Thornwave library version");
        return 0;
    }
}

extern "C" const char *tw_last_error(void) {
    return last_error.c_str();
}
