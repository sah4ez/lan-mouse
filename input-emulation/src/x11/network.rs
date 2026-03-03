use crate::x11::{X11EmulationError, X11Result, logging::timed};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime};

/// Тип события для сетевой передачи
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkEventType {
    Motion,
    Button,
    Key,
    Scroll,
}

/// Сериализованное событие для сетевой передачи
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEvent {
    /// Тип события
    pub event_type: NetworkEventType,
    /// Временная метка (миллисекунды)
    pub timestamp: u64,
    /// Данные события
    pub data: NetworkEventData,
}

impl NetworkEvent {
    /// Создать новое сетевое событие
    pub fn new(event_type: NetworkEventType, data: NetworkEventData) -> Self {
        Self {
            event_type,
            timestamp: Self::current_timestamp(),
            data,
        }
    }

    /// Получить текущую временную метку
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64
    }

    /// Проверить, устарело ли событие
    pub fn is_expired(&self, max_age: Duration) -> bool {
        let now = Self::current_timestamp();
        let age = Duration::from_millis(now - self.timestamp);
        age > max_age
    }
}

/// Данные события для сетевой передачи
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkEventData {
    Motion { dx: i32, dy: i32 },
    Button { button: u32, state: u8 },
    Key { scancode: u32, state: u8 },
    Scroll { axis: u8, value: f64 },
}

/// Пакет событий для сетевой передачи
#[derive(Debug, Clone)]
pub struct EventBatch {
    /// События в этом пакете
    pub events: Vec<NetworkEvent>,
    /// Время создания пакета
    pub created_at: Instant,
    /// Максимальный размер пакета
    pub max_size: usize,
    /// Максимальная длительность пакета
    pub max_duration: Duration,
}

impl EventBatch {
    /// Создать новый пакет событий
    pub fn new(max_size: usize, max_duration: Duration) -> Self {
        Self {
            events: Vec::with_capacity(max_size),
            created_at: Instant::now(),
            max_size,
            max_duration,
        }
    }

    /// Добавить событие в пакет
    pub fn add(&mut self, event: NetworkEvent) -> bool {
        if self.events.len() >= self.max_size {
            tracing::trace!(
                target: "x11::network::batch",
                "batch full, cannot add event"
            );
            return false;
        }

        self.events.push(event);
        true
    }

    /// Проверить, готов ли пакет к отправке
    pub fn is_ready(&self) -> bool {
        !self.events.is_empty()
            && (self.events.len() >= self.max_size
                || self.created_at.elapsed() >= self.max_duration)
    }

    /// Получить размер пакета
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Проверить, пуст ли пакет
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Очистить пакет
    pub fn clear(&mut self) {
        self.events.clear();
        self.created_at = Instant::now();

        tracing::trace!(
            target: "x11::network::batch",
            "batch cleared"
        );
    }
}

/// Конфигурация низкой задержки
#[derive(Debug, Clone)]
pub struct LatencyConfig {
    /// Максимальная допустимая задержка (миллисекунды)
    pub max_latency_ms: u64,
    /// Таймаут пакета событий (миллисекунды)
    pub batch_timeout_ms: u64,
    /// Включить алгоритм Nagle
    pub enable_nagle: bool,
    /// Интервал TCP keepalive
    pub keepalive_interval: Duration,
}

impl LatencyConfig {
    /// Создать конфигурацию по умолчанию
    pub fn new() -> Self {
        Self {
            max_latency_ms: 16,  // ~60 FPS
            batch_timeout_ms: 5, // 5ms таймаут пакета
            enable_nagle: false, // Отключить для низкой задержки
            keepalive_interval: Duration::from_secs(30),
        }
    }

    /// Получить таймаут пакета
    pub fn batch_timeout(&self) -> Duration {
        Duration::from_millis(self.batch_timeout_ms)
    }

    /// Получить максимальный размер пакета
    pub fn max_batch_size(&self) -> usize {
        // Размер пакета рассчитывается на основе максимальной задержки
        // и предполагаемого времени передачи одного события
        let events_per_ms = 1000.0 / self.max_latency_ms as f64;
        ((events_per_ms * self.batch_timeout_ms as f64) as usize).max(1)
    }
}

impl Default for LatencyConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Сериализовать событие в байты
pub fn serialize_event(event: &NetworkEvent) -> X11Result<Vec<u8>> {
    timed("x11::network::serialize", "serde_json::to_vec", || {
        serde_json::to_vec(event).map_err(|e| {
            X11EmulationError::ProtocolError(format!("Failed to serialize event: {}", e))
        })
    })
}

/// Десериализовать событие из байтов
pub fn deserialize_event(data: &[u8]) -> X11Result<NetworkEvent> {
    timed(
        "x11::network::deserialize",
        "serde_json::from_slice",
        || {
            serde_json::from_slice(data).map_err(|e| {
                X11EmulationError::ProtocolError(format!("Failed to deserialize event: {}", e))
            })
        },
    )
}

/// Создать пакет событий из списка событий
pub fn create_batch(events: Vec<NetworkEvent>, config: &LatencyConfig) -> EventBatch {
    let mut batch = EventBatch::new(config.max_batch_size(), config.batch_timeout());

    for event in events {
        if !batch.add(event) {
            tracing::warn!(
                target: "x11::network::batch",
                "batch full, dropping event"
            );
            break;
        }
    }

    tracing::debug!(
        target: "x11::network::batch",
        batch_size = batch.len(),
        max_size = config.max_batch_size(),
        "batch created"
    );

    batch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_event_creation() {
        let event = NetworkEvent::new(
            NetworkEventType::Motion,
            NetworkEventData::Motion { dx: 10, dy: 5 },
        );

        assert_eq!(event.event_type, NetworkEventType::Motion);
        assert!(event.timestamp > 0);
    }

    #[test]
    fn test_network_event_expired() {
        let event = NetworkEvent::new(
            NetworkEventType::Motion,
            NetworkEventData::Motion { dx: 10, dy: 5 },
        );

        // Event should not be expired with a large max age
        assert!(!event.is_expired(Duration::from_secs(60)));

        // Event should be expired with a very small max age (after some time)
        // Note: This test might be flaky if the system is very slow
        std::thread::sleep(Duration::from_millis(10));
        assert!(event.is_expired(Duration::from_millis(5)));
    }

    #[test]
    fn test_event_batch() {
        let mut batch = EventBatch::new(10, Duration::from_millis(5));

        assert!(batch.is_empty());
        assert!(!batch.is_ready());

        let event = NetworkEvent::new(
            NetworkEventType::Motion,
            NetworkEventData::Motion { dx: 10, dy: 5 },
        );

        assert!(batch.add(event));
        assert!(!batch.is_empty());
        // Batch is not ready yet (1 event, max 10, no timeout)
        assert!(!batch.is_ready());

        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn test_event_batch_full() {
        let mut batch = EventBatch::new(2, Duration::from_millis(100));

        let event = NetworkEvent::new(
            NetworkEventType::Motion,
            NetworkEventData::Motion { dx: 10, dy: 5 },
        );

        assert!(batch.add(event.clone()));
        assert!(batch.add(event.clone()));
        assert!(!batch.add(event)); // Should fail when batch is full

        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_event_batch_timeout() {
        let mut batch = EventBatch::new(10, Duration::from_millis(10));

        let event = NetworkEvent::new(
            NetworkEventType::Motion,
            NetworkEventData::Motion { dx: 10, dy: 5 },
        );

        assert!(batch.add(event));
        assert!(!batch.is_ready()); // Not ready yet (1 event, max 10)

        std::thread::sleep(Duration::from_millis(15));
        assert!(batch.is_ready()); // Ready due to timeout
    }

    #[test]
    fn test_event_batch_clear() {
        let mut batch = EventBatch::new(10, Duration::from_millis(5));

        let event = NetworkEvent::new(
            NetworkEventType::Motion,
            NetworkEventData::Motion { dx: 10, dy: 5 },
        );

        batch.add(event);
        assert_eq!(batch.len(), 1);

        batch.clear();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn test_latency_config() {
        let config = LatencyConfig::new();

        assert_eq!(config.max_latency_ms, 16);
        assert_eq!(config.batch_timeout_ms, 5);
        assert!(!config.enable_nagle);
        assert_eq!(config.keepalive_interval, Duration::from_secs(30));
    }

    #[test]
    fn test_latency_config_default() {
        let config = LatencyConfig::default();

        assert_eq!(config.max_latency_ms, 16);
        assert_eq!(config.batch_timeout_ms, 5);
        assert!(!config.enable_nagle);
    }

    #[test]
    fn test_latency_config_batch_timeout() {
        let config = LatencyConfig::new();

        assert_eq!(config.batch_timeout(), Duration::from_millis(5));
    }

    #[test]
    fn test_latency_config_max_batch_size() {
        let config = LatencyConfig::new();

        // Calculate expected batch size
        // events_per_ms = 1000.0 / 16 = 62.5
        // max_batch_size = (62.5 * 5) as usize = 312
        let expected_size =
            ((1000.0 / config.max_latency_ms as f64) * config.batch_timeout_ms as f64) as usize;
        assert_eq!(config.max_batch_size(), expected_size.max(1));
    }

    #[test]
    fn test_serialization() {
        let event = NetworkEvent::new(
            NetworkEventType::Motion,
            NetworkEventData::Motion { dx: 10, dy: 5 },
        );

        let serialized = serialize_event(&event).unwrap();
        let deserialized = deserialize_event(&serialized).unwrap();

        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.data, deserialized.data);
    }

    #[test]
    fn test_serialization_button() {
        let event = NetworkEvent::new(
            NetworkEventType::Button,
            NetworkEventData::Button {
                button: 1,
                state: 1,
            },
        );

        let serialized = serialize_event(&event).unwrap();
        let deserialized = deserialize_event(&serialized).unwrap();

        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.data, deserialized.data);
    }

    #[test]
    fn test_serialization_key() {
        let event = NetworkEvent::new(
            NetworkEventType::Key,
            NetworkEventData::Key {
                scancode: 42,
                state: 0,
            },
        );

        let serialized = serialize_event(&event).unwrap();
        let deserialized = deserialize_event(&serialized).unwrap();

        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.data, deserialized.data);
    }

    #[test]
    fn test_serialization_scroll() {
        let event = NetworkEvent::new(
            NetworkEventType::Scroll,
            NetworkEventData::Scroll {
                axis: 0,
                value: 1.5,
            },
        );

        let serialized = serialize_event(&event).unwrap();
        let deserialized = deserialize_event(&serialized).unwrap();

        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.data, deserialized.data);
    }

    #[test]
    fn test_create_batch() {
        let config = LatencyConfig::new();
        let events = vec![
            NetworkEvent::new(
                NetworkEventType::Motion,
                NetworkEventData::Motion { dx: 10, dy: 5 },
            ),
            NetworkEvent::new(
                NetworkEventType::Button,
                NetworkEventData::Button {
                    button: 1,
                    state: 1,
                },
            ),
        ];

        let batch = create_batch(events, &config);

        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
    }

    #[test]
    fn test_create_batch_full() {
        let config = LatencyConfig::new();
        let events = (0..config.max_batch_size() + 1)
            .map(|_| {
                NetworkEvent::new(
                    NetworkEventType::Motion,
                    NetworkEventData::Motion { dx: 1, dy: 1 },
                )
            })
            .collect();

        let batch = create_batch(events, &config);

        // Should only have max_batch_size events
        assert_eq!(batch.len(), config.max_batch_size());
    }

    #[test]
    fn test_network_event_data_equality() {
        let data1 = NetworkEventData::Motion { dx: 10, dy: 5 };
        let data2 = NetworkEventData::Motion { dx: 10, dy: 5 };
        let data3 = NetworkEventData::Motion { dx: 5, dy: 10 };

        assert_eq!(data1, data2);
        assert_ne!(data1, data3);
    }

    #[test]
    fn test_network_event_type_equality() {
        assert_eq!(NetworkEventType::Motion, NetworkEventType::Motion);
        assert_ne!(NetworkEventType::Motion, NetworkEventType::Button);
    }
}
