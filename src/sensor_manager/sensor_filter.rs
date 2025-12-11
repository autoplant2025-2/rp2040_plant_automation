use nalgebra::{U10, Matrix, Vector};

use adskalman::{KalmanFilterNoControl, TransitionModelLinearNoControl, ObservationModelLinear, StateAndCovariance};

// --- Multi-Channel Filter (10 Dimensions) ---
// 0: SHT Temp
// 1: SHT Hum
// 2: AHT Temp
// 3: AHT Hum
// 4-7: NTC 1-4
// 8: Soil
// 9: EC

type Matrix10 = Matrix<f32, U10, U10, nalgebra::ArrayStorage<f32, U10, U10>>;
type Vector10 = Vector<f32, U10, nalgebra::ArrayStorage<f32, U10, nalgebra::U1>>;

struct MultiChannelModel {
    transition_matrix: Matrix10,
    process_noise: Matrix10,
    observation_matrix: Matrix10,
    measurement_noise: Matrix10,
}

impl TransitionModelLinearNoControl<f32, U10> for MultiChannelModel {
    fn transition_model(&self) -> &Matrix10 {
        &self.transition_matrix
    }
    fn transition_model_transpose(&self) -> &Matrix10 {
        &self.transition_matrix
    }
    fn transition_noise_covariance(&self) -> &Matrix10 {
        &self.process_noise
    }
}

impl ObservationModelLinear<f32, U10, U10> for MultiChannelModel {
    fn observation_matrix(&self) -> &Matrix10 {
        &self.observation_matrix
    }
    fn observation_matrix_transpose(&self) -> &Matrix10 {
        &self.observation_matrix
    }
    fn observation_noise_covariance(&self) -> &Matrix10 {
        &self.measurement_noise
    }
    fn evaluate(&self, state: &Vector10) -> Vector10 {
        self.observation_matrix * state
    }
}

pub struct MultiChannelKalmanFilter {
    state: Vector10,
    covariance: Matrix10,
    model: MultiChannelModel,
}

impl MultiChannelKalmanFilter {
    pub fn new(initial_values: [f32; 10], process_noise: f32, measurement_noises: [f32; 10]) -> Self {
        let mut r_diag = Vector10::zeros();
        for i in 0..10 {
            r_diag[i] = measurement_noises[i];
        }

        Self {
            state: Vector10::from_column_slice(&initial_values),
            covariance: Matrix10::identity(), // Initial uncertainty
            model: MultiChannelModel {
                transition_matrix: Matrix10::identity(),
                process_noise: Matrix10::identity() * process_noise,
                observation_matrix: Matrix10::identity(),
                measurement_noise: Matrix10::from_diagonal(&r_diag),
            },
        }
    }

    pub fn update(&mut self, measurements: [f32; 10]) -> [f32; 10] {
        let filter = KalmanFilterNoControl::new(&self.model, &self.model);
        
        let observation = Vector10::from_column_slice(&measurements);
        let estimate = StateAndCovariance::new(self.state, self.covariance);
        
        let new_estimate = filter.step(&estimate, &observation);
        
        self.state = *new_estimate.state();
        self.covariance = *new_estimate.covariance();
        
        let mut result = [0.0; 10];
        for i in 0..10 {
            result[i] = self.state[i];
        }
        result
    }
}
