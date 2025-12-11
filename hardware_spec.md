# Hardware Specification

## 1. Temperature Control System
*   **Actuator**: Peltier Element
    *   **Control**: H-Bridge (Bidirectional control for heating/cooling)
*   **Sensors**:
    *   Inner Surface Temperature: NTC Thermistor (via PCF8591)
    *   Outer Surface Temperature: NTC Thermistor (via PCF8591)
*   **Cooling**:
    *   Inner Fan
    *   Outer Fan

## 2. Humidity Control System
*   **Actuator**: Peltier Element
    *   **Control**: MOSFET (Unidirectional, likely for dehumidification/cooling)
*   **Sensors**:
    *   Cold Side Temperature: NTC Thermistor (via PCF8591)
    *   Hot Side Temperature: NTC Thermistor (via PCF8591)
*   **Cooling**:
    *   Hot Side Fan

## 3. Environmental Sensors
*   **Chamber Internal**: DHT20 (Temperature & Humidity)
*   **Chamber External**: AHT20 (Temperature & Humidity)

## 4. Ventilation
*   **Actuator**: Ventilation Fan
    *   **Placement**: Directed towards the inner heatsink of the Temperature Control Peltier.

## 5. Lighting
*   **Actuator**: Plant Grow LED
    *   **Control**: MOSFET

## 6. Soil & Nutrient Monitoring
*   **Sensors**:
    *   Soil Moisture Sensor: Analog Output (via RP2040 ADC)
    *   EC (Electrical Conductivity) Sensor: Analog Output (via RP2040 ADC)

## 7. Fluid Control
*   **Actuators**:
    *   Nutrient Pump: MOSFET
    *   Water Pump: MOSFET

## 8. Interface Summary
*   **I2C**:
    *   PCF8591 (ADC for NTCs)
    *   DHT20
    *   AHT20
*   **Analog (RP2040 Internal ADC)**:
    *   Soil Moisture Sensor
    *   EC Sensor
*   **GPIO / PWM**:
    *   H-Bridge Control (Temp Peltier)
    *   MOSFET Control (Humidity Peltier, LEDs, Pumps, Fans)
