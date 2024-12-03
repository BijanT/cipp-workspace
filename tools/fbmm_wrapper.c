#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <string.h>
#include <linux/limits.h>

int main(int argc, char *argv[])
{
	char *mnt_dir;
	char *program_name;
	char *fbmm_pid_filename;
	FILE *file;
	pid_t pid;

	if (argc < 3) {
		fprintf(stderr, "Usage: fbmm_wrapper <mnt_dir> <program> [args..]\n");
		return -1;
	}

	fbmm_pid_filename = malloc(PATH_MAX);
	if (!fbmm_pid_filename) {
		fprintf(stderr, "Could not allocate buffer\n");
		return -1;
	}

	mnt_dir = argv[1];
	program_name = argv[2];

	pid = getpid();

	sprintf(fbmm_pid_filename, "/proc/%d/fbmm_mnt_dir", pid);

	file = fopen(fbmm_pid_filename, "w");
	if (!file) {
		fprintf(stderr, "Could not open %s\n", fbmm_pid_filename);
		return -1;
	}
	fprintf(file, "%s", mnt_dir);

	fclose(file);

	// execute the intended program
	execv(program_name, &argv[2]);

	// We only get here if execv fails
	fprintf(stderr, "Failed to execute %s: %s\n", program_name, strerror(errno));

	return 0;
}
