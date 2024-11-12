# build modules
obj-m = nvmemul.o
nvmemul-objs = pmc.o

# use the kernel build system
KERNEL_SOURCE := /users/bijan/kernel/kbuild

SRCDIR=`pwd`
OBJDIR=`pwd`

all:
	make -C $(KERNEL_SOURCE)  M=$(OBJDIR) modules

clean: 
	make -C $(KERNEL_SOURCE) M=$(OBJDIR) clean
