//go:build debug && darwin

package boxmain

import "syscall"

func rusageMaxRSS() float64 {
	ru := syscall.Rusage{}
	err := syscall.Getrusage(syscall.RUSAGE_SELF, &ru)
	if err != nil {
		return 0
	}

	// ru_maxrss is bytes on Darwin.
	return float64(ru.Maxrss) / (1 << 20)
}
