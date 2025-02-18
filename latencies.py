from parse import parse
import statistics
import numpy

latencies_dict = {}
latencies = []
latency_bands = {100: [], 200: [], 400: [], 800: [], 1600: [], 3200: [], 6400: []}
def add_to_latency_bands(latency, message_length):
    if message_length < 100:
        latency_bands[100].append(latency)
        return
    if message_length < 200:
        latency_bands[200].append(latency)
        return
    if message_length < 400:
        latency_bands[400].append(latency)
        return
    if message_length < 800:
        latency_bands[800].append(latency)
        return
    if message_length < 1600:
        latency_bands[1600].append(latency)
        return
    if message_length < 3200:
        latency_bands[3200].append(latency)
        return
    if message_length < 6400:
        latency_bands[6400].append(latency)
        return

with open("latencies") as f:
    for line in f.readlines():
        (message_id, action, time_str, message_length_str) = parse("(ID: {}) {} AT TIME - {} WITH SIZE {}\n", line)
        time = int(time_str)
        message_length = int(message_length_str)
        if action == "SENT":
            latencies_dict[message_id] = time
        else:
            print(message_id, time - latencies_dict[message_id])
            latency = time - latencies_dict[message_id]
            latencies.append(latency)
            add_to_latency_bands(latency, message_length)


print("==================================")
print(f"count: {len(latencies)}")
print(f"median: {statistics.median(latencies)}")
print(f"std: {statistics.stdev(latencies)}")
print(f"lower quartile: {numpy.quantile(latencies, [0.25])[0]}")
print(f"upper quartile: {numpy.quantile(latencies, [0.75])[0]}")
print(f"90th quantile: {numpy.quantile(latencies, [0.90])[0]}")
print(f"99th quantile: {numpy.quantile(latencies, [0.995])[0]}")
print(f"min: {numpy.quantile(latencies, [0])[0]}")
print(f"max: {numpy.quantile(latencies, [1])[0]}")
for k in latency_bands:
    if len(latency_bands[k]) == 0:
        continue
    print("==================================")
    print(f"message length band: {k}")
    print(f"count: {len(latency_bands[k])}")
    print(f"median: {statistics.median(latency_bands[k])}")
    print(f"std: {statistics.stdev(latency_bands[k])}")
    print(f"lower quartile: {numpy.quantile(latency_bands[k], [0.25])[0]}")
    print(f"upper quartile: {numpy.quantile(latency_bands[k], [0.75])[0]}")
    print(f"90th quantile: {numpy.quantile(latency_bands[k], [0.90])[0]}")
    print(f"99th quantile: {numpy.quantile(latency_bands[k], [0.995])[0]}")
    print(f"min: {numpy.quantile(latency_bands[k], [0])[0]}")
    print(f"max: {numpy.quantile(latency_bands[k], [1])[0]}")
print("==================================")
