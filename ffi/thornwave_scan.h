#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct TwScanner TwScanner;

typedef struct TwAdvertisement {
    uint64_t serial;
    uint64_t address_raw;

    uint32_t device_time_raw;
    uint32_t flags_raw;

    float voltage1_volts;
    float voltage2_volts;
    float current_amps;
    float power_watts;
    float coulomb_meter_raw;
    float power_meter_raw;
    float temperature_celsius;

    uint16_t firmware_version_bcd;
    uint8_t hardware_revision_bcd;

    uint8_t power_status_code;
    uint8_t soc_raw;
    uint16_t runtime_raw;
    int16_t rssi_dbm;

    bool temperature_is_external;

    char name[64];
    char model[64];
    char address_display[64];
    char power_status_display[64];
} TwAdvertisement;

typedef void (*TwAdvertisementCallback)(void *user_data, const TwAdvertisement *advertisement);

TwScanner *tw_scanner_create(void);
void tw_scanner_destroy(TwScanner *scanner);

int tw_scanner_set_callback(TwScanner *scanner, TwAdvertisementCallback callback, void *user_data);

int tw_scanner_start_ble(TwScanner *scanner);

void tw_scanner_stop_ble(TwScanner *scanner);

uint16_t tw_library_version_bcd(void);
const char *tw_last_error(void);

#ifdef __cplusplus
}
#endif
