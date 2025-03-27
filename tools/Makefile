# GNR is default, for haswell: make ARCH=haswell
ARCH=gnr

cflags.common=-Wall -Werror -std=c++20
cflags.haswell=-DHASWELL
cflags.gnr=-DGNR
CFLAGS := ${cflags.common} ${cflags.${ARCH}}

all: fbmm_wrapper cipp cipp_total_bw bwmon memlat meminfo
	echo "DONE"

fbmm_wrapper: fbmm_wrapper.c
	gcc fbmm_wrapper.c -o fbmm_wrapper

cipp: cipp.o perf.o
	g++ $^ -o $@

cipp_total_bw: cipp_total_bw.o perf.o
	g++ $^ -o $@

bwmon: bwmon.o perf.o
	g++ $^ -o $@

memlat: memlat.o perf.o
	g++ $^ -o $@ -lnuma

meminfo: meminfo.cpp
	g++ $^ -o $@

cipp.o: cipp.cpp perf.h
	g++ $(CFLAGS) cipp.cpp -c -o $@

cipp_total_bw.o: cipp_total_bw.cpp perf.h
	g++ $(CFLAGS) cipp_total_bw.cpp -c -o $@

bwmon.o: bwmon.cpp perf.h
	g++ $(CFLAGS) bwmon.cpp -c -o $@

memlat.o: memlat.cpp perf.h
	g++ $(CFLAGS) memlat.cpp -c -o $@

perf.o: perf.cpp perf.h
	g++ $(CFLAGS) perf.cpp -c -o $@

clean:
	rm -rf bwmon cipp fbmm_wrapper meminfo *.o
