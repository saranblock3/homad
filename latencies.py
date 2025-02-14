from parse import parse
import statistics
import numpy

latencies_dict = {}
latencies = []

with open("latencies") as f:
    for line in f.readlines():
        (message_id, action, time_str) = parse("(ID: {}) {} AT TIME - {}\n", line)
        time = int(time_str)
        if action == "SENT":
            latencies_dict[message_id] = time
        else:
            print(message_id, time - latencies_dict[message_id])
            latencies.append(time - latencies_dict[message_id])

print(f"count: {len(latencies)}")
print(f"median: {statistics.median(latencies)}")
print(f"std: {statistics.stdev(latencies)}")
print(f"lower quartile: {numpy.quantile(latencies, [0.25])[0]}")
print(f"upper quartile: {numpy.quantile(latencies, [0.75])[0]}")
print(f"90th quantile: {numpy.quantile(latencies, [0.90])[0]}")
print(f"99th quantile: {numpy.quantile(latencies, [0.995])[0]}")
print(f"min: {numpy.quantile(latencies, [0])[0]}")
print(f"max: {numpy.quantile(latencies, [1])[0]}")
