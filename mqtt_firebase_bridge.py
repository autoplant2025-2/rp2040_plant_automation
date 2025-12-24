import json
import time
import requests
import paho.mqtt.client as mqtt
from datetime import datetime

# Configuration
FIREBASE_URL = "https://plantdatabase-1adf2-default-rtdb.asia-southeast1.firebasedatabase.app/"
MQTT_BROKER = "127.0.0.1" # Localhost (run on PC)
MQTT_PORT = 1883

# Paths
FIREBASE_CONFIG_PATH = "settings.json"
FIREBASE_LOGS_PATH = "logs.json"

# MQTT Client
client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)

def on_connect(client, userdata, flags, rc, properties):
    print(f"Connected to MQTT Broker with result code {rc}")
    client.subscribe("plant/logs")
    client.subscribe("plant/config") # Just in case we want to echo back?

def on_message(client, userdata, msg):
    try:
        topic = msg.topic
        payload = msg.payload.decode()
        
        if topic == "plant/logs":
            print(f"Received Log: {payload}")
            # Ensure it's valid JSON
            data = json.loads(payload)
            # Push to Firebase
            # We use POST to append to a list, or PUT/PATCH to specific ID.
            # Logs are a list usually?
            # Firebase Realtime DB: POST to /logs.json generates unique ID.
            url = f"{FIREBASE_URL}logs.json"
            resp = requests.post(url, json=data)
            if resp.status_code == 200:
                print("Log synced to Firebase")
            else:
                print(f"Failed to sync log: {resp.status_code} {resp.text}")
                
    except Exception as e:
        print(f"Error processing message: {e}")

client.on_connect = on_connect
client.on_message = on_message

# Connect MQTT
try:
    client.connect(MQTT_BROKER, MQTT_PORT, 60)
    client.loop_start()
except Exception as e:
    print(f"Failed to connect to MQTT Broker: {e}")
    exit(1)

# Polling Loop for Config Changes (PC -> Plant)
# Ideally we use Firebase Streaming API (SSE) for valid real-time updates.
# But polling is simpler for a script.
# Let's poll every 5 seconds.

last_config_str = ""

print("Bridge Service Started...")

while True:
    try:
        # Fetch Config
        url = f"{FIREBASE_URL}{FIREBASE_CONFIG_PATH}"
        resp = requests.get(url)
        if resp.status_code == 200:
            current_config = resp.json()
            # Normalize or just compare JSON strings?
            # Be careful with key ordering.
            current_str = json.dumps(current_config, sort_keys=True)
            
            if current_str != last_config_str:
                if last_config_str != "": # Don't update on first run to avoid boot loop spam? Or do?
                     print("Config Changed on Firebase! Sending to Plant...")
                     # Publish to MQTT
                     # We might need to flatten it or ensure it matches `ConfigUpdate` struct on Rust.
                     # Rust expects fields like `target_temp`, `plant_name`.
                     # Let's assume Firebase structure matches Rust structure directly under `settings`.
                     # If Firebase returns `null`, handle it.
                     if current_config:
                         client.publish("plant/config", json.dumps(current_config), retain=True)
                     else:
                         print("Config is empty/null")
                         
                last_config_str = current_str
        else:
            print(f"Failed to fetch config: {resp.status_code}")
            
    except Exception as e:
        print(f"Error in polling loop: {e}")
        
    time.sleep(5)
