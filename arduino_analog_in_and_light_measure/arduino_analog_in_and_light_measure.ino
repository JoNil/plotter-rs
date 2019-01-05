#include <stdint.h>

const int lightSensorInPin = A0;
const int causeDigitalInPin = 9;

void setup()
{
    Serial.begin(250000);
    pinMode(lightSensorInPin, INPUT);
    pinMode(causeDigitalInPin, INPUT_PULLUP);
}

enum class CauseState : uint8_t {
    Low  = 0b00000000,
    High = 0b01000000,
};

CauseState cause_state = CauseState::Low;

void loop()
{    
    uint32_t light_level = analogRead(lightSensorInPin);

    if (digitalRead(causeDigitalInPin) == HIGH) {
        cause_state = CauseState::High;
    } else {
        cause_state = CauseState::Low;
    }

    uint8_t high = 0b10000000 | (uint8_t)cause_state | ((light_level >> 5) & 0b00011111);
    uint8_t low  = 0b00000000 |                        (light_level & 0b00011111);

    Serial.write(low);
    Serial.write(high);

    uint32_t now = micros();

    uint32_t time_send = now >> 4;
    
    Serial.write((time_send)       & 0x7f);
    Serial.write((time_send >> 7)  & 0x7f);
    Serial.write((time_send >> 14) & 0x7f);
    Serial.write((time_send >> 21) & 0x7f);
}
