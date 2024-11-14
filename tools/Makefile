CFLAGS=-Wall -Werror -std=c++20

all: cipp bwmon meminfo
	echo "DONE"

cipp: cipp.o perf.o
	g++ $^ -o $@

bwmon: bwmon.o perf.o
	g++ $^ -o $@

meminfo: meminfo.cpp
	g++ $^ -o $@

cipp.o: cipp.cpp
	g++ $(CFLAGS) cipp.cpp -c -o $@

bwmon.o: bwmon.cpp perf.h
	g++ $(CFLAGS) bwmon.cpp -c -o $@

perf.o: perf.cpp perf.h
	g++ $(CFLAGS) perf.cpp -c -o $@

clean:
	rm -rf bwmon cipp meminfo *.o
