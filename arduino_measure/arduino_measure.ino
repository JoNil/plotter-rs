#include <stdint.h>

const int analogInPin = A0;
const int analogOutPin = 9;

void setup()
{
    Serial.begin(250000);
}

enum class LightState : uint8_t {
    Off = 0b00000000,
    On  = 0b01000000,
};

LightState light_state = LightState::Off;
uint32_t light_next_change_time = 0;

volatile uint32_t time_1 = 0;
volatile uint32_t time_2 = 0;
volatile uint32_t time_3 = 0;

void loop()
{
    uint32_t last ;
    uint32_t now = micros();
    uint32_t rng = random(0, 100000);

    if (now > light_next_change_time) {

        if (light_state == LightState::Off) {

            analogWrite(analogOutPin, 0); // turn on light
            light_state = LightState::On;

        } else {
            analogWrite(analogOutPin, 255); // turn off light
            light_state = LightState::Off;
        }

        time_1 = now;
        time_2 = 400000;
        time_3 = rng;

    } else {

        if (light_state == LightState::Off) {

            analogWrite(analogOutPin, 255); // turn off light
            light_state = LightState::Off;

        } else {
            analogWrite(analogOutPin, 0); // turn on light
            light_state = LightState::On;
        }

        time_1 = light_next_change_time;
        time_2 = 0;
        time_3 = 0;
    }

    light_next_change_time = time_1 + time_2 + time_3;
    
    uint32_t light_level = analogRead(analogInPin);

    uint8_t high = 0b10000000 | (uint8_t)light_state | ((light_level >> 5) & 0b00011111);
    uint8_t low  = 0b00000000 |                        (light_level & 0b00011111);

    Serial.write(low);
    Serial.write(high);

    uint32_t time_send = now >> 4;
    
    Serial.write((time_send)       & 0x7f);
    Serial.write((time_send >> 7)  & 0x7f);
    Serial.write((time_send >> 14) & 0x7f);
    Serial.write((time_send >> 21) & 0x7f);
}

