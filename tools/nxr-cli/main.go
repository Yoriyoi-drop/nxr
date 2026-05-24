package main

import (
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
)

const defaultAddr = "127.0.0.1:9643"

func main() {
	if len(os.Args) < 2 {
		printUsage()
		os.Exit(1)
	}

	addr := os.Getenv("NXR_ADDR")
	if addr == "" {
		addr = defaultAddr
	}

	cmd := os.Args[1]
	args := os.Args[2:]

	switch cmd {
	case "query", "q":
		runQuery(addr, strings.Join(args, " "))
	case "vinsert", "vi":
		runVInsert(addr, args)
	case "vsearch", "vs":
		runVSearch(addr, args)
	case "gadd":
		runGAdd(addr, args)
	case "gget":
		runGGet(addr, args)
	case "kvget":
		runKvGet(addr, args)
	case "kvset":
		runKvSet(addr, args)
	case "stats":
		runStats(addr)
	case "init":
		fmt.Println("nxr-db init <path>")
	case "help", "--help", "-h":
		printUsage()
	default:
		fmt.Printf("Unknown command: %s\n", cmd)
		printUsage()
		os.Exit(1)
	}
}

func printUsage() {
	fmt.Println(`NXR CLI - AI-native database tools

Usage:
  nxr query "MATCH ..."      Run NXR-QL query
  nxr vinsert <id> <dim> <vals...>  Insert vector
  nxr vsearch <val1,val2,...> Search similar vectors
  nxr gadd <label> <k=v,...> Add graph node
  nxr gget <id>               Get graph node
  nxr kvget <key>             Get KV value
  nxr kvset <key> <val> <ttl> Set KV value
  nxr stats                   Database statistics
  nxr help                    This help

Environment:
  NXR_ADDR   Server address (default: 127.0.0.1:9643)
`)
}

func sendCommand(addr, cmd string) string {
	conn, err := net.Dial("tcp", addr)
	if err != nil {
		return fmt.Sprintf("Connection error: %v", err)
	}
	defer conn.Close()

	_, err = fmt.Fprintf(conn, "%s\n", cmd)
	if err != nil {
		return fmt.Sprintf("Write error: %v", err)
	}

	buf := make([]byte, 65536)
	n, err := conn.Read(buf)
	if err != nil {
		return fmt.Sprintf("Read error: %v", err)
	}

	return strings.TrimSpace(string(buf[:n]))
}

func runQuery(addr, query string) {
	if query == "" {
		fmt.Println("Error: empty query")
		os.Exit(1)
	}
	result := sendCommand(addr, "QUERY "+query)
	fmt.Println(result)
}

func runVInsert(addr string, args []string) {
	if len(args) < 3 {
		fmt.Println("Usage: nxr vinsert <id> <dim> <val1 val2 ...>")
		os.Exit(1)
	}
	cmd := "VINSERT " + strings.Join(args, " ")
	result := sendCommand(addr, cmd)
	fmt.Println(result)
}

func runVSearch(addr string, args []string) {
	if len(args) < 1 {
		fmt.Println("Usage: nxr vsearch <val1,val2,...>")
		os.Exit(1)
	}
	result := sendCommand(addr, "VSEARCH "+strings.Join(args, ","))
	fmt.Println(result)
}

func runGAdd(addr string, args []string) {
	if len(args) < 1 {
		fmt.Println("Usage: nxr gadd <label> [k=v,...]")
		os.Exit(1)
	}
	label := args[0]
	props := ""
	if len(args) > 1 {
		props = strings.Join(args[1:], " ")
	}
	cmd := fmt.Sprintf("GADD %s %s", label, props)
	result := sendCommand(addr, cmd)
	fmt.Println(result)
}

func runGGet(addr string, args []string) {
	if len(args) < 1 {
		fmt.Println("Usage: nxr gget <id>")
		os.Exit(1)
	}
	_ = args[0]
	fmt.Println("gget: not implemented via simple protocol")
}

func runKvGet(addr string, args []string) {
	if len(args) < 1 {
		fmt.Println("Usage: nxr kvget <key>")
		os.Exit(1)
	}
	result := sendCommand(addr, "KVGET "+args[0])
	fmt.Println(result)
}

func runKvSet(addr string, args []string) {
	if len(args) < 2 {
		fmt.Println("Usage: nxr kvset <key> <value> [ttl]")
		os.Exit(1)
	}
	ttl := "0"
	if len(args) > 2 {
		ttl = args[2]
	}
	cmd := fmt.Sprintf("KVSET %s %s %s", args[0], args[1], ttl)
	result := sendCommand(addr, cmd)
	fmt.Println(result)
}

func runStats(addr string) {
	result := sendCommand(addr, "STATS")
	fmt.Println(result)
}

func init() {
	// stubs for unused imports
	_ = strconv.Itoa
}
