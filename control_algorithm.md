# Control Algorithm Design (Revised v4)

## 1. Temperature Control: Cascade PID + Pre-conditioning Feedforward
The ventilation fan is directed at the Peltier's inner heatsink. This creates a **"Pre-conditioning"** effect where incoming air is heated/cooled before mixing with the chamber air.

### Feedforward Logic (The "Load Compensation")
We must calculate the thermal power required to bring the incoming fresh air to the target chamber temperature.

$$ Effort_{FF\_Vent} = K_{vent} \times Speed_{vent} \times (T_{target} - T_{external}) $$

*   **$T_{target}$**: The desired chamber temperature.
*   **$T_{external}$**: The outside air temperature (AHT20).
*   **$Speed_{vent}$**: Current ventilation fan speed (0.0 - 1.0).
*   **$K_{vent}$**: A calibration constant representing the heat capacity of the airflow.

**Example Scenario:**
*   Target: 25°C, External: 10°C (Cold).
*   Vent Fan turns ON.
*   Formula: $K \times Speed \times (25 - 10) = +15K$.
*   **Result**: The PID adds heating power **immediately**. The Peltier heats the cold fresh air as it passes through the heatsink, so it enters the chamber at ~25°C. No temperature drop occurs.

## 2. Fan Interaction Strategy
Since the Vent Fan blows at the heatsink, it contributes to the convection cooling of the Peltier.

### Logic: "Airflow Summation"
The Peltier efficiency depends on total airflow.
$$ Airflow_{total} \approx Speed_{inner\_fan} + \alpha \times Speed_{vent} $$

*   **Scenario A (High Thermal Demand)**:
    *   PID requests High Cooling.
    *   Inner Fan = Max.
    *   If Vent Fan is also ON (for CO2), Total Airflow is Boosted. -> **Better Performance**.
*   **Scenario B (Low Thermal Demand, High Vent)**:
    *   PID requests Low Cooling.
    *   Vent Fan is ON (for CO2).
    *   The high airflow makes the Peltier very efficient.
    *   The PID will naturally reduce the Peltier Power (PWM) to prevent overshooting. **No special code needed here**, the feedback loop handles the efficiency change.

## 3. Humidity Control
*   Unchanged.

## 4. Summary
*   **Key Feature**: The Feedforward term turns the ventilation system into an "Active Air Intake" that pre-treats the air.
*   **Requirement**: Accurate External Temperature Sensor ($T_{ext}$) is critical for this to work.
