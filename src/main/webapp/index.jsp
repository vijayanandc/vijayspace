<!DOCTYPE html>
<%@page import="java.util.Iterator"%>
<%@page import="com.vijay.TestUtil"%>
<%@page import="java.util.ArrayList"%>
<html lang="en">
<head>
  <title>Bootstrap Case</title>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" href="http://maxcdn.bootstrapcdn.com/bootstrap/3.3.6/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/1.12.0/jquery.min.js"></script>
  <script src="http://maxcdn.bootstrapcdn.com/bootstrap/3.3.6/js/bootstrap.min.js"></script>
</head>
<body>

<nav class="navbar navbar-default">
  <div class="container-fluid">
    <div class="navbar-header">
      <a class="navbar-brand" href="#">Vijay Space</a>
    </div>
    <ul class="nav navbar-nav">
    <%
    	ArrayList<String> tabNames = TestUtil.getTabNames();
    	Iterator<String> iterator = tabNames.iterator();
    	while(iterator.hasNext()){
    		String tabName = iterator.next();
    		String className = "";
    		if("Home".equals(tabName)){
    			className = "active";
    		}
    %>
      <li class="<%=className%>"><a href="#" onclick="alert('<%=tabName%> :)')"><%=tabName%></a></li>
      <%
    	}      
      %>
      
    </ul>
  </div>
</nav>
  
<div class="container">
  <h3>Basic Navbar Example</h3>
  <p>A navigation bar is a navigation header that is placed at the top of the page.</p>
</div>

</body>
</html>
