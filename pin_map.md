# RP2040 Pin Mapping
계획짤때 사용하고 업데이트를 안해서 실제와 다릅니다.

## User Interface
| Function | Pin | Notes |
| :--- | :--- | :--- |
| Encoder A | GPIO 0 | |
| Encoder B | GPIO 1 | |
| Encoder Btn | GPIO 2 | |
| LCD CLK | GPIO 14 | SPI1 |
| LCD MOSI | GPIO 15 | SPI1 |

## Communication
| Function | Pin | Notes |
| :--- | :--- | :--- |
| I2C SDA | GPIO 20 | I2C0 (Sensors) |
| I2C SCL | GPIO 21 | I2C0 (Sensors) |

## Actuators (PWM/GPIO)
| Function | Pin | Type | Notes |
| :--- | :--- | :--- | :--- |
| Fan Inner | GPIO 3 | PWM | Slice 1 B |
| Fan Vent | GPIO 4 | PWM | Slice 2 A |
| LED | GPIO 5 | PWM | Slice 2 B |
| Temp Peltier A | GPIO 6 | PWM | Slice 3 A (H-Bridge) |
| Temp Peltier B | GPIO 7 | PWM | Slice 3 B (H-Bridge) |
| Hum Peltier | GPIO 8 | PWM | Slice 4 A |
| Pump Nutrient | GPIO 9 | GPIO | |
| Pump Water | GPIO 10 | GPIO | |

## Analog Sensors (Internal ADC)
| Function | Pin | ADC Channel | Notes |
| :--- | :--- | :--- | :--- |
| Soil Moisture | GPIO 26 | ADC 0 | |
| EC Sensor | GPIO 27 | ADC 1 | |
