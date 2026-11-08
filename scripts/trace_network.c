/* Linux x86-64 release-check helper: trace a command and every descendant.
 * Reject Internet socket creation/activation and io_uring (uninspected here).
 * Unix-domain desktop IPC is allowed. Requires ptrace access to own children.
 * Build: cc -O2 -Wall -Wextra -Werror scripts/trace_network.c -o /tmp/nameh-trace
 * Run: /tmp/nameh-trace /tmp/network.log target/release/nameh FILE
 */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/audit.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ptrace.h>
#include <linux/ptrace.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: %s NEW_LOG COMMAND [ARG...]\n", argv[0]);
        return 2;
    }
    FILE *log = fopen(argv[1], "wx");
    if (!log) { perror("log"); return 2; }
    pid_t root = fork();
    if (root < 0) { perror("fork"); return 2; }
    if (root == 0) {
        fclose(log);
        if (ptrace(PTRACE_TRACEME, 0, 0, 0) < 0) { perror("ptrace"); _exit(2); }
        raise(SIGSTOP);
        execvp(argv[2], argv + 2);
        perror("exec");
        _exit(2);
    }
    int status, failed = 0, root_status = 2;
    if (waitpid(root, &status, 0) != root || !WIFSTOPPED(status)) return 2;
    unsigned long options = PTRACE_O_TRACESYSGOOD | PTRACE_O_TRACECLONE |
        PTRACE_O_TRACEFORK | PTRACE_O_TRACEVFORK | PTRACE_O_TRACEEXEC | PTRACE_O_EXITKILL;
    if (ptrace(PTRACE_SETOPTIONS, root, 0, options) < 0) { perror("options"); return 2; }
    if (ptrace(PTRACE_SYSCALL, root, 0, 0) < 0) return 2;
    for (;;) {
        pid_t pid = waitpid(-1, &status, __WALL);
        if (pid < 0) {
            if (errno == EINTR) continue;
            if (errno != ECHILD) { perror("waitpid"); failed = 1; }
            break;
        }
        if (WIFEXITED(status) || WIFSIGNALED(status)) {
            if (pid == root) root_status = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
            continue;
        }
        if (!WIFSTOPPED(status)) continue;
        int sig = WSTOPSIG(status), deliver = 0;
        if (sig == (SIGTRAP | 0x80)) {
            struct ptrace_syscall_info info = {0};
            if (ptrace(PTRACE_GET_SYSCALL_INFO, pid, sizeof(info), &info) < 0) {
                perror("syscall info"); failed = 1;
            } else if (info.op == PTRACE_SYSCALL_INFO_ENTRY) {
                if (info.arch != AUDIT_ARCH_X86_64) {
                    fprintf(log, "UNINSPECTED architecture pid=%d\n", pid); failed = 1;
                } else if (info.entry.nr == SYS_socket || info.entry.nr == SYS_socketpair) {
                    unsigned long long family = info.entry.args[0];
                    fprintf(log, "socket pid=%d family=%llu\n", pid, family);
                    if (family != AF_UNIX) failed = 1;
                } else if (info.entry.nr == SYS_connect || info.entry.nr == SYS_sendto) {
                    unsigned long long address = info.entry.args[info.entry.nr == SYS_connect ? 1 : 4];
                    if (address) {
                        errno = 0;
                        long word = ptrace(PTRACE_PEEKDATA, pid, (void *)address, 0);
                        if (errno) { fprintf(log, "UNINSPECTED address pid=%d\n", pid); failed = 1; }
                        else {
                            unsigned family = (unsigned short)word;
                            fprintf(log, "destination pid=%d family=%u\n", pid, family);
                            if (family != AF_UNIX && family != AF_UNSPEC) failed = 1;
                        }
                    }
                } else if (info.entry.nr == SYS_io_uring_setup) {
                    fprintf(log, "UNINSPECTED io_uring pid=%d\n", pid); failed = 1;
                }
            }
            fflush(log);
        } else if (sig != SIGTRAP && sig != SIGSTOP) deliver = sig;
        if (ptrace(PTRACE_SYSCALL, pid, 0, deliver) < 0 && errno != ESRCH) {
            perror("resume"); failed = 1;
        }
    }
    fprintf(log, "root_exit=%d forbidden_or_uninspected=%d\n", root_status, failed);
    fclose(log);
    return failed ? 1 : root_status;
}
