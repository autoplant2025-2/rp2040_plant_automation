# Control Loop Logic (Pseudocode)

## 1. Declarations & Initialization

### Hardware Handles
*   `i2c_bus`: Shared I2C bus.
*   `dht20`: Chamber Internal Temp/Hum Sensor.
*   `aht20`: Chamber External Temp/Hum Sensor.
*   `pcf8591`: ADC for NTC Thermistors (Inner/Outer/Cold/Hot).
*   `adc_soil`: Internal ADC for Soil Moisture.
*   `adc_ec`: Internal ADC for EC.
*   `pwm_peltier_temp`: H-Bridge Control (Dir + PWM).
*   `pwm_peltier_hum`: MOSFET Control (PWM).
*   `pwm_fan_inner`: Inner Circulation Fan.
*   `pwm_fan_outer`: Outer Heatsink Fan.
*   `pwm_fan_vent`: Ventilation Fan.
*   `pwm_led`: Plant Grow LED.

### State Variables
*   `current_time`: RTC Time.
*   `plant_day`: Days since start.
*   `target_state`: Struct { temp, humidity, light_on, vent_mode, ... }
*   `sensor_data`: Struct { temp_in, hum_in, temp_out, temp_peltier_in, ... }
*   `pid_temp_outer`: PID Instance (Air -> Peltier Temp).
*   `pid_temp_inner`: PID Instance (Peltier Temp -> PWM).
*   `pid_hum`: PID Instance (Humidity -> PWM).

## 2. Main Control Loop (Runs every X ms)

### Step 1: Read Sensors
1.  **Environment**:
    *   `sensor_data.internal` = `dht20.read()`
    *   `sensor_data.external` = `aht20.read()`
2.  **Components**:
    *   `raw_ntc` = `pcf8591.read_all()`
    *   Convert `raw_ntc` to Celsius -> `sensor_data.peltier_inner`, `sensor_data.peltier_outer`, etc.
3.  **Soil/Water**:
    *   `sensor_data.soil` = `adc_soil.read()`

### Step 2: Determine Targets (Rhai Script)
1.  Update `script_engine` context with `current_time`, `plant_day`, `sensor_data`.
2.  **Execute Script**: `let targets = script.eval("get_targets()")`
3.  **Extract**: `target_temp`, `target_hum`, `target_vent_speed`, `led_status`.

### Step 3: Humidity Control Logic
1.  **Error Calc**: `error_hum = target_hum - sensor_data.internal.humidity`
2.  **PID Run**: `hum_effort = pid_hum.compute(error_hum)`
3.  **Output**:
    *   If `hum_effort < 0` (Dehumidify): `hum_peltier_pwm = abs(hum_effort)`, `hum_fan_speed = High`
    *   If `hum_effort > 0` (Humidify): `humidifier_pwm = hum_effort` (if exists)

### Step 4: Ventilation Logic
1.  **Source**: `vent_speed` is directly provided by the Rhai script (`target_vent_speed`).
2.  **Override**: The script can set it to 0 (off), low (base), or high (flush) based on time/logic.

### Step 5: Temperature Control Logic (Cascade + FF)
1.  **Feedforward Calculation**:
    *   `ff_hum` = `K_hum * hum_peltier_pwm` (Compensate for Hum Peltier cooling)
    *   `ff_vent` = `K_vent * vent_speed * (target_temp - sensor_data.external.temp)` (Compensate for incoming air)
2.  **Outer Loop (Air Temp)**:
    *   `error_temp = target_temp - sensor_data.internal.temp`
    *   `temp_effort_raw = pid_temp_outer.compute(error_temp)`
    *   `total_effort = temp_effort_raw + ff_hum + ff_vent`
3.  **Splitter (Effort -> Actuators)**:
    *   **Fan**: `target_fan_speed = Base + (K_fan * abs(total_effort))`
    *   **Peltier Target**:
        *   Map `total_effort` (-100% to +100%) to Range (0°C to 60°C).
        *   `target_peltier_temp = map(total_effort, -100, 100, 0, 60)`
4.  **Inner Loop (Peltier Surface)**:
    *   `error_peltier = target_peltier_temp - sensor_data.peltier_inner`
    *   `peltier_pwm = pid_temp_inner.compute(error_peltier)`

### Step 6: Post-Processing & Safety
1.  **Fan Smoothing**:
    *   `final_fan_inner = slew_limiter.update(target_fan_speed)`
    *   `final_fan_vent = slew_limiter_vent.update(vent_speed)`
2.  **Noise Limit**: Clamp fan speeds to User Max Limit.
3.  **Safety Stop**:
    *   If `sensor_data.peltier_inner > 70°C` -> **EMERGENCY STOP** (PWM=0).

### Step 7: Actuate Hardware
1.  `pwm_peltier_temp.set(peltier_pwm)`
2.  `pwm_peltier_hum.set(hum_peltier_pwm)`
3.  `pwm_fan_inner.set(final_fan_inner)`
4.  `pwm_fan_vent.set(final_fan_vent)`
5.  `pwm_led.set(led_status ? 100% : 0%)`

### Step 8: Logging
1.  Push `sensor_data` and `targets` to History Buffer.
2.  Wait for next cycle.
